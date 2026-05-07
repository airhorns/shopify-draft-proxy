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
    captureId: 'b2b-quantity-rules-extended-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-quantity-rules-extended-validation-conformance.mts',
    purpose:
      'B2B-backed price-list quantityRulesAdd maximum-vs-existing-price-break validation, missing-variant validation, and quantityRulesDelete no-existing-rule validation.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}quantity-rules-extended-validation.json`,
      'config/parity-specs/b2b/quantity-rules-extended-validation.json',
      'config/parity-requests/markets/quantity-pricing-by-variant-update.graphql',
      'config/parity-requests/markets/quantity-rules-add-validation.graphql',
      'config/parity-requests/markets/quantity-rules-delete.graphql',
    ],
    cleanupBehavior:
      'Deletes any existing fixed quantity pricing for the configured variant, records validation failures, seeds a disposable quantity price break, captures the overlap validation failure, then deletes the seeded fixed quantity pricing.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
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
      'config/parity-requests/b2b/b2b-staff-assignment-validation-assign-unknown.graphql',
      'config/parity-requests/b2b/b2b-staff-assignment-validation-create.graphql',
      'config/parity-requests/b2b/b2b-staff-assignment-validation-read-after-unknown.graphql',
      'config/parity-requests/b2b/b2b-staff-assignment-validation-remove-unknown.graphql',
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
      'config/parity-requests/b2b/b2b-bulk-field-paths-assign-staff.graphql',
      'config/parity-requests/b2b/b2b-bulk-field-paths-companies-delete.graphql',
      'config/parity-requests/b2b/b2b-bulk-field-paths-company-create.graphql',
      'config/parity-requests/b2b/b2b-bulk-field-paths-contact-assign-roles.graphql',
      'config/parity-requests/b2b/b2b-bulk-field-paths-contact-create.graphql',
      'config/parity-requests/b2b/b2b-bulk-field-paths-contact-revoke-roles.graphql',
      'config/parity-requests/b2b/b2b-bulk-field-paths-contacts-delete.graphql',
      'config/parity-requests/b2b/b2b-bulk-field-paths-location-assign-roles.graphql',
      'config/parity-requests/b2b/b2b-bulk-field-paths-location-create.graphql',
      'config/parity-requests/b2b/b2b-bulk-field-paths-location-revoke-roles.graphql',
      'config/parity-requests/b2b/b2b-bulk-field-paths-locations-delete.graphql',
      'config/parity-requests/b2b/b2b-bulk-field-paths-remove-staff.graphql',
    ],
    cleanupBehavior:
      'Creates disposable B2B companies, contacts, locations, and role assignments; bulk-delete/revoke scenario steps remove most setup records and the script deletes the primary company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Staff assignment evidence uses Shopify validation for an invalid StaffMember input shape because the current conformance token cannot read staffMembers/currentStaffMember IDs.',
  },
  {
    domain: 'b2b',
    captureId: 'b2b-revoke-role-scope-preconditions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-revoke-role-scope-conformance.mts',
    purpose:
      'B2B contact/location revoke-role parent lookup, wrong-scope assignment validation, empty-id precondition, and partial-success semantics.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-revoke-role-scope-preconditions.json`,
      'config/parity-specs/b2b/b2b-revoke-role-scope-preconditions.json',
      'config/parity-requests/b2b/b2b-revoke-role-scope-company-create.graphql',
      'config/parity-requests/b2b/b2b-revoke-role-scope-contact-assign-roles.graphql',
      'config/parity-requests/b2b/b2b-revoke-role-scope-contact-create.graphql',
      'config/parity-requests/b2b/b2b-revoke-role-scope-contact-revoke-role.graphql',
      'config/parity-requests/b2b/b2b-revoke-role-scope-contact-revoke-roles.graphql',
      'config/parity-requests/b2b/b2b-revoke-role-scope-location-assign-roles.graphql',
      'config/parity-requests/b2b/b2b-revoke-role-scope-location-create.graphql',
      'config/parity-requests/b2b/b2b-revoke-role-scope-location-revoke-roles.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable B2B company with a secondary contact and extra locations, assigns contact/location roles, records revoke validation branches, and deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Public Admin 2026-04 returns RESOURCE_NOT_FOUND for wrong-scope contact revoke assignment IDs and a null field/null revokedRoleAssignmentIds payload for empty roleAssignmentIds with revokeAll false.',
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
      'config/parity-requests/b2b/contact-role-cascade-assign-role.graphql',
      'config/parity-requests/b2b/contact-role-cascade-company-create.graphql',
      'config/parity-requests/b2b/contact-role-cascade-location-create.graphql',
      'config/parity-requests/b2b/contact-role-cascade-locations-read.graphql',
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
      'config/parity-requests/b2b/b2b-string-validation-company-create.graphql',
      'config/parity-requests/b2b/b2b-string-validation-location-create.graphql',
    ],
    cleanupBehavior:
      'Creates one setup company for child mutation validation plus cleanup for any live branch that unexpectedly creates a company.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The capture intentionally records live HTML mismatch probes so reviewers can distinguish executable parity-backed validation branches from current Admin behavior that does not reproduce the internal B2B change rules.',
  },
  {
    domain: 'b2b',
    captureId: 'b2b-address-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-address-validation-conformance.mts',
    purpose:
      'B2B CompanyAddressInput country, zone, zip, HTML, emoji, and name URL validation branches for location create, assign-address, and nested company-create location inputs.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-address-validation.json`,
      'config/parity-specs/b2b/b2b-address-validation.json',
      'config/parity-requests/b2b/b2b-address-validation-assign-address.graphql',
      'config/parity-requests/b2b/b2b-address-validation-company-create.graphql',
      'config/parity-requests/b2b/b2b-address-validation-location-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable B2B company with a location for validation targets, records resolver userErrors that do not create additional records, then deletes the setup company.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      '`CompanyLocationUpdateInput` does not expose address fields in the public Admin GraphQL schema on the live capture target; update-path address validation remains runtime-test-backed.',
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
      'config/parity-requests/b2b/b2b-company-update-customer-since-create.graphql',
      'config/parity-requests/b2b/b2b-company-update-customer-since-read.graphql',
      'config/parity-requests/b2b/b2b-company-update-customer-since-update.graphql',
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
      'config/parity-requests/b2b/external-id-validation-company-create.graphql',
      'config/parity-requests/b2b/external-id-validation-company-update.graphql',
      'config/parity-requests/b2b/external-id-validation-location-create.graphql',
      'config/parity-requests/b2b/external-id-validation-location-update.graphql',
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
      'config/parity-requests/b2b/b2b-location-address-management-address-delete.graphql',
      'config/parity-requests/b2b/b2b-location-address-management-assign-address.graphql',
      'config/parity-requests/b2b/b2b-location-address-management-create.graphql',
      'config/parity-requests/b2b/b2b-location-address-management-location-create.graphql',
      'config/parity-requests/b2b/b2b-location-address-management-location-delete.graphql',
      'config/parity-requests/b2b/b2b-location-address-management-read-location-delete.graphql',
      'config/parity-requests/b2b/b2b-location-address-management-read-shared-delete.graphql',
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
      'config/parity-requests/b2b/b2b-contact-business-rules-assign-role.graphql',
      'config/parity-requests/b2b/b2b-contact-business-rules-company-create.graphql',
      'config/parity-requests/b2b/b2b-contact-business-rules-company-read.graphql',
      'config/parity-requests/b2b/b2b-contact-business-rules-contact-delete.graphql',
      'config/parity-requests/b2b/b2b-contact-business-rules-draft-order-complete.graphql',
      'config/parity-requests/b2b/b2b-contact-business-rules-draft-order-create.graphql',
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
      'config/parity-requests/b2b/b2b-no-input-validation-company-create.graphql',
      'config/parity-requests/b2b/b2b-no-input-validation-company-read.graphql',
      'config/parity-requests/b2b/b2b-no-input-validation-company-update.graphql',
      'config/parity-requests/b2b/b2b-no-input-validation-contact-create.graphql',
      'config/parity-requests/b2b/b2b-no-input-validation-contact-update.graphql',
      'config/parity-requests/b2b/b2b-no-input-validation-location-update.graphql',
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
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-change-status-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-input-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-then-bulk-create-price-range-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-multi-value-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-with-options-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-delete-async-operation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-duplicate-async-missing.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-duplicate-async-success.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-feedback-mutation-access-blockers.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-feeds-empty-read.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-handle-dedup-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-handle-validation-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-helper-roots-read.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-inline-synthetic-id-read.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-invalid-search-query-syntax.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-media-validation-branches.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-merchandising-mutation-probes.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-metafields.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-option-update-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-limits-and-duplicates-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-create-over-default-limit.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-create-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-leave-as-is-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-null-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-delete-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-related-by-id-not-found.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-reorder-media-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-async-operation-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-duplicate-variants-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-options-only-requires-variants.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-set-shape-validator-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-update-tag-normalization.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-user-error-shape-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variant-relationship-bulk-update-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variants-bulk-reorder-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-variants-bulk-validation-atomicity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-contextual-pricing-price-list-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-relationship-roots.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-variant-media-validation.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-change-status-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-change-status-unknown-product-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-inventory-read-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-media-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-create-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-delete-media-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-delete-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-detail.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-duplicate-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-empty-state.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-metafields.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-option-update-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-options-create-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-options-delete-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-publish-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-set-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-unpublish-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-blank-title-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-media-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-update-unknown-id-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-create-inventory-read-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-create-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-delete-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-bulk-update-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/product-variants-matrix.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/products-variant-search.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-advanced-search.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-catalog-page.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-or-precedence.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-relevance-search.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search-grammar.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search-pagination.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-search.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/products-sort-keys.json',
    ],
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
      'config/parity-requests/products/product-invalid-search-query-create.graphql',
      'config/parity-requests/products/product-invalid-search-query-search.graphql',
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
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-create-parity.json`,
      `${CAPTURE_ROOT}product-update-parity.json`,
      `${CAPTURE_ROOT}product-delete-parity.json`,
    ],
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
      'config/parity-requests/products/productUserErrorShape-collectionCreate.graphql',
      'config/parity-requests/products/productUserErrorShape-inventoryActivate.graphql',
      'config/parity-requests/products/productUserErrorShape-productCreate.graphql',
      'config/parity-requests/products/productUserErrorShape-productOptionsCreate.graphql',
      'config/parity-requests/products/productUserErrorShape-productOptionsDelete.graphql',
      'config/parity-requests/products/productUserErrorShape-productVariantsBulkReorder.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no Shopify objects should be created.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'combined-listing-update-validation',
    scriptPath: 'scripts/capture-combined-listing-update-validation-conformance.ts',
    purpose:
      'combinedListingUpdate parent role, child relation, optionsAndValues, duplicate, missing child, already-child, edit/remove overlap, and title validation payloads.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}combinedListingUpdate-validation.json`,
      'config/parity-specs/products/combinedListingUpdate-validation.json',
      'config/parity-requests/products/combinedListingUpdate-validation-product-create.graphql',
      'config/parity-requests/products/combinedListingUpdate-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable PARENT, plain, and child products; records validation failures plus setup success branches; deletes all setup products during cleanup.',
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
    captureId: 'product-bundle-create-validation',
    scriptPath: 'scripts/capture-product-bundle-create-validation-conformance.ts',
    purpose:
      'productBundleCreate component product lookup, option mapping, quantity maximum, quantityOption, consolidatedOptions, and ProductBundleOperation readback behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}productBundleCreate-validation.json`,
      'config/parity-specs/products/productBundleCreate-validation.json',
      'config/parity-requests/products/productBundleCreate-validation.graphql',
      'config/parity-requests/products/productBundleOperation-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable component product with product options; bundle validation branches create no products and the setup product is deleted in best-effort cleanup.',
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
    captureId: 'selling-plan-group-add-remove-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-selling-plan-group-add-remove-validation-conformance.ts',
    purpose:
      'Selling-plan group add/remove product and product-variant validation for unknown ids, unknown groups, duplicate membership, known non-member removal, and malformed removal ids.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}selling-plan-group-add-remove-validation.json`,
      'config/parity-specs/products/sellingPlanGroup-add-remove-validation.json',
    ],
    cleanupBehavior:
      'Creates disposable products and a disposable selling-plan group, records validation branches, then deletes them in best-effort cleanup.',
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
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/tags-add-multi-resource.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/tags-add-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/tags-remove-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/tags-add-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/tags-remove-parity.json',
    ],
    cleanupBehavior: 'Creates temporary products and resets/deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'tags-add-multi-resource',
    scriptPath: 'scripts/capture-tags-add-multi-resource-conformance.ts',
    purpose:
      'tagsAdd/tagsRemove polymorphic Product, Order, Customer, Article, DraftOrder, and unsupported-GID behavior.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_orders',
      'write_orders',
      'read_customers',
      'write_customers',
      'read_content',
      'write_content',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}tags-add-multi-resource.json`,
      'config/parity-specs/products/tagsAdd-multi-resource.json',
    ],
    cleanupBehavior:
      'Creates disposable product, customer, order, draft order, blog, and article records, tags them, then deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-publications',
    scriptPath: 'scripts/capture-product-publication-conformance.mts',
    purpose: 'Publication aggregate reads plus productPublish/productUnpublish probes.',
    requiredAuthScopes: ['read_products', 'write_products', 'publication/channel access for the app'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/store-properties/publishable-publish-current-shop-count-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/store-properties/publishable-publish-shop-count-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/store-properties/publishable-unpublish-current-shop-count-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/store-properties/publishable-unpublish-shop-count-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/publications-catalog.json',
    ],
    cleanupBehavior: 'Publishes/unpublishes disposable products only after publication target probes pass.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'products',
    captureId: 'product-publish-input-validation',
    scriptPath: 'scripts/capture-product-publish-input-validation-conformance.ts',
    purpose:
      'productPublish ProductPublicationInput validation for omitted lists, empty lists, and unknown publication/channel targets.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}productPublish-input-validation.json`,
      'config/parity-specs/products/productPublish-input-validation.json',
      'config/parity-requests/products/productPublish-input-validation.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable draft product, records validation branches, captures a hydration cassette while the product exists, then deletes the product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-media-mutations',
    scriptPath: 'scripts/capture-product-media-mutation-conformance.mts',
    purpose: 'Product media create/update/delete validation and downstream read branches.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-media-validation-branches.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productCreateMedia-dual-userErrors.json',
      'config/parity-requests/products/productCreateMedia-dual-userErrors.graphql',
      'config/parity-specs/products/product-media-validation-branches.json',
      'config/parity-specs/products/productCreateMedia-dual-userErrors.json',
    ],
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
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/media/file-acknowledge-update-failed-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/media/file-create-delete-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/media/file-delete-product-media-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/media/file-update-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/media/media-file-cascade-variant-media-clear.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/media/media-file-create-then-image-read.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/media/media-file-create-validation-branches.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/media/media-file-delete-typed-gid-roundtrip.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/media/media-file-update-validation-branches.json',
      `${LOCAL_RUNTIME_ROOT}files-upload-local-runtime.json`,
    ],
    cleanupBehavior:
      'Deletes created files when Shopify returns file IDs; local-runtime fixtures need no Shopify cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'media-file-cascade-variant-media-clear',
    scriptPath: 'scripts/capture-media-file-cascade-variant-media-clear-conformance.mts',
    purpose:
      'Files API fileDelete and fileUpdate.referencesToRemove cascades that clear ProductVariant media membership after removing product media associations.',
    requiredAuthScopes: ['read_files', 'write_files', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-cascade-variant-media-clear.json`,
      'config/parity-specs/media/media-file-cascade-variant-media-clear.json',
      'config/parity-requests/media/media-file-cascade-file-delete.graphql',
      'config/parity-requests/media/media-file-cascade-file-update-remove-reference.graphql',
      'config/parity-requests/media/media-file-cascade-variant-media-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable products with image media attached to their default variants; deletes the products and any detached file left by the update scenario during cleanup.',
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
    captureId: 'staged-upload-user-errors-shape',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-staged-upload-user-errors-shape-conformance.ts',
    purpose: 'stagedUploadsCreate UserError field/message shape and schema rejection for selecting code.',
    requiredAuthScopes: ['write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-staged-uploads-create-user-errors-shape.json`,
      'config/parity-specs/media/media-staged-uploads-create-user-errors-shape.json',
    ],
    cleanupBehavior: 'Requests validation-only staged upload metadata and creates no Shopify files.',
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
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-options-create-parity.json`,
      `${CAPTURE_ROOT}product-option-update-parity.json`,
      `${CAPTURE_ROOT}product-options-delete-parity.json`,
      `${CAPTURE_ROOT}product-options-create-variant-strategy-create-parity.json`,
    ],
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
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-create-over-default-limit.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-create-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-leave-as-is-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-create-variant-strategy-null-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-default-custom-standalone.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-default-default-standalone.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-remove-custom-standalone.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/productVariantsBulkCreate-strategy-remove-default-standalone.json',
      'config/parity-specs/products/productOptionsCreate-variant-strategy-create-over-default-limit.json',
      'config/parity-specs/products/productOptionsCreate-variant-strategy-create.json',
      'config/parity-specs/products/productOptionsCreate-variant-strategy-leave-as-is.json',
      'config/parity-specs/products/productOptionsCreate-variant-strategy-null.json',
      'config/parity-specs/products/productVariantsBulkCreate-strategy-default-custom-standalone.json',
      'config/parity-specs/products/productVariantsBulkCreate-strategy-default-default-standalone.json',
      'config/parity-specs/products/productVariantsBulkCreate-strategy-remove-custom-standalone.json',
      'config/parity-specs/products/productVariantsBulkCreate-strategy-remove-default-standalone.json',
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
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-variants-bulk-update-parity.json`,
      `${CAPTURE_ROOT}product-variants-bulk-create-parity.json`,
      `${CAPTURE_ROOT}product-variants-bulk-delete-parity.json`,
    ],
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
      'config/parity-requests/products/productVariantsBulkCreate-validation-atomicity.graphql',
      'config/parity-requests/products/productVariantsBulkCreate-validation-options.graphql',
      'config/parity-requests/products/productVariantsBulkCreate-validation.graphql',
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
      'config/parity-requests/products/productCreate-then-bulkCreate-derived-bulk-create.graphql',
      'config/parity-requests/products/productCreate-then-bulkCreate-derived-create.graphql',
      'config/parity-requests/products/productCreate-then-bulkCreate-derived-downstream.graphql',
      'config/parity-requests/products/productCreate-then-bulkCreate-derived-price-update.graphql',
      'config/parity-requests/products/inventoryAdjust-then-hasOutOfStockVariants-adjust.graphql',
      'config/parity-requests/products/inventoryAdjust-then-hasOutOfStockVariants-downstream.graphql',
      'config/parity-requests/products/inventoryAdjust-then-hasOutOfStockVariants-setup.graphql',
    ],
    cleanupBehavior:
      'Creates disposable products for price-range and inventory aggregate captures, then deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-transfers',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-inventory-transfer-conformance.ts',
    purpose:
      'inventoryTransferCreate validation and inventory transfer draft-to-ready-to-canceled lifecycle behavior with downstream inventory reservation readback.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_inventory',
      'write_inventory',
      'read_locations',
      'write_locations',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-transfer-create-validation.json`,
      `${CAPTURE_ROOT}inventory-transfer-lifecycle-local-staging.json`,
      'config/parity-specs/products/inventory_transfer_create_validation.json',
      'config/parity-specs/products/inventory-transfer-lifecycle-local-staging.json',
      'config/parity-requests/products/inventory-transfer-create-validation.graphql',
      'config/parity-requests/products/inventory-transfer-create.graphql',
      'config/parity-requests/products/inventory-transfer-mark-ready.graphql',
      'config/parity-requests/products/inventory-transfer-inventory-read-all-levels.graphql',
      'config/parity-requests/products/inventory-transfer-cancel.graphql',
      'config/parity-requests/products/inventory-transfer-delete.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable locations and one disposable tracked product, activates inventory at both locations, records validation and lifecycle branches, cancels the ready transfer, attempts the captured non-draft delete guardrail, then deletes the product and locations.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-item-mutations',
    scriptPath: 'scripts/capture-inventory-item-mutation-conformance.mts',
    purpose: 'inventoryItemUpdate and product-backed inventory item mutation behavior.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_inventory', 'write_inventory'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/inventory-item-update-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/inventory-item-update-validation.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/inventory-item-update-parity.json',
    ],
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
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/metafields/metafields-set-input-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-cas-success-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-duplicate-input-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-key-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-namespace-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-owner-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-type-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-value-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-null-create-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-over-limit-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-owner-expansion-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-stale-digest-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/metafields/metafields-set-parity.json',
      `${CAPTURE_ROOT}metafields-delete-parity.json`,
    ],
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
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-pinning.json`,
      `${CAPTURE_ROOT}metafield-definition-pinning-parity.json`,
    ],
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
    captureId: 'metafield-definition-create-with-pin-guards',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-create-with-pin-guards.mts',
    purpose:
      'metafieldDefinitionCreate(pin: true) pin-limit and constrained-definition validation, plus constrained standardMetafieldDefinitionEnable(pin: true).',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-create-with-pin-guards.json`,
      'config/parity-specs/metafields/metafield-definition-create-with-pin-guards.json',
      'config/parity-requests/metafields/metafield-definition-create-with-pin-guards.graphql',
      'config/parity-requests/metafields/metafield-definition-create-with-pin-guards-read.graphql',
    ],
    cleanupBehavior:
      'Temporarily unpins existing product definitions, creates disposable product-owned definitions, deletes them, then restores original pins.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The live standard-enable pin-cap branch currently creates an unpinned definition on the 2026-04 target, so this capture records the constrained standard-enable branch while runtime tests cover the ticket-required cap behavior.',
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
      'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-create.graphql',
      'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-delete-no-flag.graphql',
      'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-delete-with-flag.graphql',
      'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-metafields-set.graphql',
      'config/parity-requests/metafields/metafield-definition-update-delete-preconditions-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable products and product-owned definitions, deletes definitions during the scenario, then deletes any remaining definitions and products.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-update-constraints',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-update-constraints.mts',
    purpose:
      'metafieldDefinitionUpdate constraintsUpdates staging, constrained pin guard, unconstrain, and pin-after-unconstrain readback.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-update-constraints.json`,
      'config/parity-specs/metafields/metafield-definition-update-constraints.json',
      'config/parity-requests/metafields/metafield-definition-update-constraints-read.graphql',
      'config/parity-requests/metafields/metafield-definition-update-constraints.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product-owned definition, updates its constraints, then deletes any remaining definitions in the temporary namespace.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-app-namespace-resolution',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-app-namespace-resolution-conformance.mts',
    purpose:
      'metafieldDefinition app namespace resolution for create, update, identifier reads, canonical delete, and cross-app access denial.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-app-namespace-resolution.json`,
      'config/parity-specs/metafields/metafield-definition-app-namespace-resolution.json',
      'config/parity-requests/metafields/metafield-definition-app-namespace-create.graphql',
      'config/parity-requests/metafields/metafield-definition-app-namespace-delete.graphql',
      'config/parity-requests/metafields/metafield-definition-app-namespace-read.graphql',
      'config/parity-requests/metafields/metafield-definition-app-namespace-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product-owned metafield definition in the active app namespace, deletes it during the scenario, and deletes it by id during cleanup if capture fails before canonical delete.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-graph-mutations',
    scriptPath: 'scripts/capture-product-graph-mutation-conformance.mts',
    purpose: 'Product graph mutation branches that span product/options/variants/media.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [`${CAPTURE_ROOT}product-set-parity.json`, `${CAPTURE_ROOT}product-duplicate-parity.json`],
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
      'config/parity-specs/products/productDuplicate-async-missing.json',
      'config/parity-specs/products/productDuplicate-async-success.json',
    ],
    cleanupBehavior: 'Creates disposable source/duplicate products and deletes both after operation completion.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-delete-async',
    scriptPath: 'scripts/capture-product-delete-async-conformance.ts',
    purpose: 'Asynchronous productDelete operation payload, duplicate pending-operation guard, and helper reads.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-delete-async-operation.json`,
      'config/parity-specs/products/productDelete-async-operation.json',
      'config/parity-requests/products/productDelete-async-operation.graphql',
      'config/parity-requests/products/productDelete-async-product-read.graphql',
      'config/parity-requests/products/productDelete-async-source-create.graphql',
      'config/parity-requests/products/productDelete-operation-node-read.graphql',
      'config/parity-requests/products/productDelete-operation-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product, enqueues async deletion, captures immediate reads, then waits for Shopify to delete it or falls back to best-effort synchronous delete.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'product-inventory-reads',
    scriptPath: 'scripts/capture-product-inventory-read-conformance.mts',
    purpose: 'Product-adjacent inventory read shapes and linkage baselines.',
    requiredAuthScopes: ['read_products', 'read_inventory', 'read_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-create-inventory-read-parity.json`,
      `${CAPTURE_ROOT}product-variants-bulk-create-inventory-read-parity.json`,
    ],
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
      'config/parity-requests/saved-searches/saved-search-resource-roots-create.graphql',
      'config/parity-requests/saved-searches/saved-search-resource-roots-delete.graphql',
      'config/parity-requests/saved-searches/saved-search-resource-roots-read.graphql',
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
      'config/parity-requests/saved-searches/saved-search-query-grammar-delete.graphql',
      'config/parity-requests/saved-searches/saved-search-query-grammar-read-after-create.graphql',
      'config/parity-requests/saved-searches/saved-search-query-grammar-validation-create.graphql',
      'config/parity-requests/saved-searches/saved-search-query-grammar-validation-update.graphql',
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
      'config/parity-requests/saved-searches/saved-search-query-grammar-validation-create.graphql',
      'config/parity-requests/saved-searches/saved-search-query-grammar-validation-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product saved search for positive/update validation and deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-unknown-filter-field',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-unknown-filter-field-conformance.ts',
    purpose:
      'SavedSearch per-resource unknown-filter validation for PRODUCT create plus known-filter positive control.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-unknown-filter-field.json`,
      'config/parity-specs/saved-searches/saved-search-unknown-filter-field.json',
      'config/parity-requests/saved-searches/saved-search-unknown-filter-field.graphql',
    ],
    cleanupBehavior: 'Creates one disposable product saved search for positive-control validation and deletes it.',
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
      'config/parity-requests/saved-searches/saved-search-delete-shop-payload-delete.graphql',
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
      'config/parity-requests/saved-searches/saved-search-name-uniqueness-update-conflict.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable product saved searches, captures duplicate create/update validation, then deletes both records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-reserved-name',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-reserved-name-conformance.ts',
    purpose: 'savedSearchCreate and savedSearchUpdate reject per-resource reserved names case-insensitively.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_orders',
      'write_orders',
      'read_draft_orders',
      'write_draft_orders',
      'read_files',
      'write_files',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-reserved-name.json`,
      'config/parity-specs/saved-searches/saved-search-reserved-name.json',
    ],
    cleanupBehavior:
      'Captures validation-only reserved-name create branches, creates one positive-control product saved search, captures a reserved-name update rejection, then deletes the positive-control record.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes: 'CUSTOMER reserved-name create behavior is deferred to the saved-search customer deprecation flow.',
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
      'config/parity-requests/saved-searches/saved-search-required-input-empty-query-create.graphql',
      'config/parity-requests/saved-searches/saved-search-required-input-missing-id-update.graphql',
      'config/parity-requests/saved-searches/saved-search-required-input-missing-name-create.graphql',
      'config/parity-requests/saved-searches/saved-search-required-input-missing-resource-type-create.graphql',
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
    captureId: 'product-variant-media-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-product-variant-media-validation-conformance.ts',
    purpose:
      'productVariantAppendMedia and productVariantDetachMedia validation for cross-product variants, cross-product media, non-ready media, and unattached detach targets.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-variant-media-validation.json`,
      'config/parity-specs/products/product_variant_append_media_validation.json',
      'config/parity-requests/products/product-variant-media-validation-append.graphql',
      'config/parity-requests/products/product-variant-media-validation-detach.graphql',
      'config/parity-requests/products/product-variant-media-validation-product-create-media.graphql',
      'config/parity-requests/products/product-variant-media-validation-product-create.graphql',
      'config/parity-requests/products/product-variant-media-validation-product-update-media.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable products plus disposable product media, then deletes both products during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-variant-relationship-bulk-update-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-product-variant-relationship-bulk-update-validation-conformance.ts',
    purpose:
      'productVariantRelationshipBulkUpdate parent/child semantics validation for parent-as-child, quantity bounds, duplicate inputs, exactly-one-parent-id, and update-not-child branches.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-variant-relationship-bulk-update-validation.json`,
      'config/parity-specs/products/productVariantRelationshipBulkUpdate-validation.json',
      'config/parity-requests/products/productVariantRelationshipBulkUpdate-validation-product-create.graphql',
      'config/parity-requests/products/productVariantRelationshipBulkUpdate-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable parent/child products, marks the parent variant as requiring components, captures validation probes, then deletes the products.',
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
    domain: 'products',
    captureId: 'selling-plan-group-input-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-selling-plan-group-input-validation-conformance.ts',
    purpose: 'Selling-plan group create/update input validation for group limits and nested selling-plan guardrails.',
    requiredAuthScopes: ['read_products', 'write_products', 'write_purchase_options'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}selling-plan-group-input-validation.json`,
      'config/parity-specs/products/sellingPlanGroupCreate-input-validation.json',
      'config/parity-requests/products/sellingPlanGroupCreate-input-validation.graphql',
      'config/parity-requests/products/sellingPlanGroupUpdate-input-validation.graphql',
    ],
    cleanupBehavior: 'Creates one disposable selling-plan group, then deletes it during cleanup.',
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
    captureId: 'metafield-definition-capability-eligibility',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-capability-eligibility.mts',
    purpose:
      'Metafield definition capability eligibility, required uniqueValues, and PRODUCT admin-filterable owner limit behavior.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-capability-eligibility.json`,
      'config/parity-specs/metafields/metafield-definition-capability-eligibility.json',
      'config/parity-requests/metafields/metafield-definition-capability-eligibility.graphql',
    ],
    cleanupBehavior:
      'Creates disposable PRODUCT metafield definitions in one namespace, captures validation and limit branches, then deletes the created definitions.',
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
      'config/parity-requests/metaobjects/metaobject-definition-create-validation-create.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-create-validation-read-by-type.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-create-validation-update.graphql',
    ],
    cleanupBehavior:
      'Validation branches create no records; successful app-prefixed and duplicate-case setup definitions are deleted during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-definition-create-field-validations',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-field-validations-conformance.ts',
    purpose:
      'Metaobject definition create fieldDefinitions validation for reserved keys, duplicate input, displayNameKey resolution, hyphen keys, and max field count.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}definition-create-field-validations.json`,
      'config/parity-specs/metaobjects/definition_create_field_validations.json',
      'config/parity-requests/metaobjects/definition-create-field-validations.graphql',
    ],
    cleanupBehavior:
      'Validation branches create no records; the successful hyphen-key definition is deleted during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-definition-name-type-description-length',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-name-type-description-length-conformance.ts',
    purpose:
      'Metaobject definition create/update validation for name presence, name/description length, and type minimum length.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}definition-name-type-description-length.json`,
      'config/parity-specs/metaobjects/definition_name_type_description_length.json',
      'config/parity-requests/metaobjects/definition-name-type-description-length-create.graphql',
      'config/parity-requests/metaobjects/definition-name-type-description-length-update.graphql',
    ],
    cleanupBehavior:
      'Create validation branches create no records; the setup definition used for update validation is deleted during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-definition-update-immutable',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-update-immutable-conformance.ts',
    purpose:
      'metaobjectDefinitionUpdate IMMUTABLE guardrails for standard definitions, reserved Shopify prefixes, and definitions linked to product options.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobjectDefinitionUpdate-immutable.json`,
      'config/parity-specs/metaobjects/metaobjectDefinitionUpdate-immutable.json',
      'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-immutable-read.graphql',
      'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-immutable-update.graphql',
    ],
    cleanupBehavior:
      'Enables a standard definition, probes reserved-prefix creation, creates disposable linked product-option setup records, captures immutable update responses, then deletes disposable setup records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The linked-product-options branch is captured as live evidence while local runtime support remains limited until product option state tracks linked metafield metadata.',
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-definition-update-capability-invariants',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-capability-invariants-conformance.ts',
    purpose:
      'metaobjectDefinitionUpdate public capability-disable behavior and renderable enable field-reference validation.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects', 'read_translations', 'write_translations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobjectDefinitionUpdate-capability-invariants.json`,
      'config/parity-specs/metaobjects/metaobjectDefinitionUpdate-capability-invariants.json',
      'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-capability-invariants-definition-create.graphql',
      'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-capability-invariants-entry-create.graphql',
      'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-capability-invariants-read.graphql',
      'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-capability-invariants-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable definitions and metaobjects for each capability branch, registers one translation for the translatable branch, captures update and read-after-update evidence, then deletes disposable records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The parity spec strictly compares public renderable enable validation. The live fixture also records public capability-disable behavior; source-backed conservative local disable guards are covered by focused Gleam runtime tests.',
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
      'config/parity-requests/metaobjects/metaobject-definition-delete-cascade-entry-create.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-delete-cascade-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable definition and two rows, deletes the definition during the scenario, then best-effort deletes any remaining rows/definition during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-update-error-codes',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-update-error-codes-conformance.ts',
    purpose:
      'metaobjectUpdate bad-id RECORD_NOT_FOUND, duplicate metaobjectCreate fields[] key validation, and non-display-field update displayName preservation.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-update-error-codes.json`,
      'config/parity-specs/metaobjects/metaobject_update_error_codes.json',
      'config/parity-requests/metaobjects/metaobject-update-error-codes-display-update.graphql',
      'config/parity-requests/metaobjects/metaobject-update-error-codes-duplicate-create.graphql',
      'config/parity-requests/metaobjects/metaobject-update-error-codes-update-bad-id.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable metaobject definition and one row; deletes the row and definition during cleanup.',
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
      'config/parity-requests/metaobjects/standard-metaobject-definition-enable-catalog.graphql',
      'config/parity-requests/metaobjects/standard-metaobject-definition-enable-read.graphql',
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
      'config/parity-requests/metaobjects/metaobject-field-validation-matrix-create.graphql',
      'config/parity-requests/metaobjects/metaobject-field-validation-matrix-definition-create.graphql',
      'config/parity-requests/metaobjects/metaobject-field-validation-matrix-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable metaobject definition and setup entry; rejected branches create no rows except captured scalar coercion branches, which are deleted during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-handle-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-handle-validation-conformance.ts',
    purpose:
      'metaobjectCreate, metaobjectUpdate, and metaobjectUpsert explicit handle format, length, and blank validation.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject_handle_validation.json`,
      'config/parity-specs/metaobjects/metaobject_handle_validation.json',
      'config/parity-requests/metaobjects/metaobject_handle_validation_create.graphql',
      'config/parity-requests/metaobjects/metaobject_handle_validation_definition_create.graphql',
      'config/parity-requests/metaobjects/metaobject_handle_validation_update.graphql',
      'config/parity-requests/metaobjects/metaobject_handle_validation_upsert.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable metaobject definition and one valid row; rejected validation branches create no rows, then cleanup deletes the row and definition.',
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
      'config/parity-requests/metaobjects/metaobject-bulk-delete-edge-both-type-and-ids.graphql',
      'config/parity-requests/metaobjects/metaobject-bulk-delete-edge-empty-ids.graphql',
      'config/parity-requests/metaobjects/metaobject-bulk-delete-edge-known-empty-type.graphql',
      'config/parity-requests/metaobjects/metaobject-bulk-delete-edge-unknown-type.graphql',
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
      `${CAPTURE_ROOT}inventory-adjust-quantities-parity.json`,
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
    captureId: 'localization-shop-locale-enable-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-shop-locale-enable-validation-conformance.mts',
    purpose:
      'shopLocaleEnable unsupported-locale, duplicate-locale, max-locale validation plus shopLocaleUpdate market-web-presence-only missing-locale behavior.',
    requiredAuthScopes: ['read_markets', 'read_locales', 'write_locales'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-shop-locale-enable-validation.json`,
      'config/parity-specs/localization/localization-shop-locale-enable-validation.json',
      'config/parity-requests/localization/localization-shop-locale-update-market-web-presences.graphql',
    ],
    cleanupBehavior:
      'Temporarily disables pre-existing alternate locales, enables disposable alternates to reach the locale cap, records validation branches, disables captured locales, and restores the pre-capture alternate locales.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-translations-mutation-noop-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-translations-mutation-noop-validation-conformance.mts',
    purpose:
      'translationsRemove unknown-key and disabled-locale no-op success behavior plus translationsRegister primary-locale validation.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-translations-mutation-noop-validation.json`,
      'config/parity-specs/localization/localization-translations-mutation-noop-validation.json',
      'config/parity-requests/localization/localization-translations-mutation-noop-validation-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product, temporarily enables and disables Italian for the disabled-locale removal branch, deletes the product, and restores Italian only if it was enabled before capture.',
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
      'config/parity-requests/localization/localization-payload-shapes-shop-locale-enable.graphql',
      'config/parity-requests/localization/localization-payload-shapes-shop-locales-read.graphql',
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
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/markets-baseline.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/markets-catalog.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/markets/markets-resolved-values.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/markets/markets-baseline.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/markets/markets-catalog.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/markets/markets-resolved-values.json',
      `${CAPTURE_ROOT}market-catalog-detail.json`,
      `${CAPTURE_ROOT}market-catalogs.json`,
      `${CAPTURE_ROOT}market-detail.json`,
      `${CAPTURE_ROOT}market-web-presences.json`,
      `${CAPTURE_ROOT}price-list-detail.json`,
      `${CAPTURE_ROOT}price-list-prices-filtered.json`,
      `${CAPTURE_ROOT}price-lists.json`,
    ],
    cleanupBehavior:
      'Read/validation oriented; do not run market lifecycle writes without disposable setup and cleanup.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'markets',
    captureId: 'markets-legacy-parity-fixtures',
    scriptPath: 'scripts/capture-market-orphan-fixture-replacements-conformance.mts',
    purpose:
      'Markets, market-localization, web-presence, quantity-pricing, and price-list parity fixtures that predate recorder provenance metadata.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_products', 'write_products', 'read_translations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}quantity-pricing-rules-parity.json`,
      `${CAPTURE_ROOT}catalog-create-missing-context.json`,
      `${CAPTURE_ROOT}catalog-lifecycle-validation.json`,
      `${CAPTURE_ROOT}market-create-status-enabled-mismatch.json`,
      `${CAPTURE_ROOT}market-localizable-empty-read.json`,
      `${CAPTURE_ROOT}market-localization-validation.json`,
      `${CAPTURE_ROOT}market-localizations-register-too-many-keys.json`,
      `${CAPTURE_ROOT}market-web-presence-delete-parity.json`,
      `${CAPTURE_ROOT}market-web-presence-validation.json`,
      `${CAPTURE_ROOT}price-list-create-dkk.json`,
      `${CAPTURE_ROOT}price-list-fixed-prices-by-product-update-parity.json`,
      `${CAPTURE_ROOT}price-list-mutation-validation.json`,
    ],
    cleanupBehavior:
      'Records validation-only branches in place, creates and deletes disposable product, web-presence, price-list, and fixed-price state for success-path captures, and removes staged quantity-pricing rows after the 2025-01 quantity capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
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
    captureId: 'price-list-fixed-prices-by-product-update-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-price-list-fixed-prices-by-product-update-validation-conformance.ts',
    purpose:
      'priceListFixedPricesByProductUpdate validation branches for no-op input, currency mismatch, duplicate product IDs, and add/delete mutual exclusivity.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}price-list-fixed-prices-by-product-update-validation.json`,
      'config/parity-specs/markets/price-list-fixed-prices-by-product-update-validation.json',
      'config/parity-requests/markets/price-list-fixed-prices-by-product-update-validation.graphql',
    ],
    cleanupBehavior:
      'Validation-only mutations reject before changing price-list fixed prices; no setup or cleanup records are created.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'price-list-input-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-price-list-input-validation-conformance.ts',
    purpose:
      'priceListCreate and priceListUpdate parent adjustment value validation plus catalog-linked currency mismatch acceptance.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}price-list-input-validation.json`,
      'config/parity-specs/markets/price-list-input-validation.json',
      'config/parity-requests/markets/price-list-input-validation-markets-read.graphql',
      'config/parity-requests/markets/price-list-create-input-validation.graphql',
      'config/parity-requests/markets/price-list-update-input-validation.graphql',
      'config/parity-requests/markets/catalog-create-relation-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable price lists and market catalogs for success/update paths, records validation-only failures that do not create records, then deletes all created price lists and catalogs.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Public Admin GraphQL 2026-04 accepts zero percentage-decrease adjustments and catalog-linked price-list currency mismatches.',
  },
  {
    domain: 'markets',
    captureId: 'catalog-relation-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-catalog-relation-validation-conformance.mts',
    purpose: 'Catalog price-list/publication relation validation for unknown ids and one-catalog relation exclusivity.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}catalog-relation-validation.json`,
      'config/parity-specs/markets/catalog-create-price-list-not-found.json',
      'config/parity-specs/markets/catalog-create-price-list-taken.json',
      'config/parity-specs/markets/catalog-update-publication-not-found.json',
      'config/parity-requests/markets/catalog-relation-markets-read.graphql',
      'config/parity-requests/markets/catalog-create-relation-validation.graphql',
      'config/parity-requests/markets/catalog-update-relation-validation.graphql',
      'config/parity-requests/markets/price-list-create-catalog-validation.graphql',
    ],
    cleanupBehavior:
      'Uses an existing market context, creates a disposable price list and setup catalogs, captures rejected relation validation branches, then deletes the disposable catalogs; attached price lists may already be removed by catalog cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'market-update-linkage',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-market-update-linkage-conformance.mts',
    purpose:
      'marketUpdate catalogsToAdd linkage lifecycle, downstream Market.catalogs and MarketCatalog.markets readback, and unknown catalog/web-presence add validation.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-update-linkage.json`,
      'config/parity-specs/markets/market-update-linkage.json',
      'config/parity-requests/markets/market-update-linkage-catalog-create.graphql',
      'config/parity-requests/markets/market-update-linkage-catalog-read.graphql',
      'config/parity-requests/markets/market-update-linkage-market-create.graphql',
      'config/parity-requests/markets/market-update-linkage-market-read.graphql',
      'config/parity-requests/markets/market-update-linkage-update.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable markets and one disposable market catalog, links the catalog to the target market, captures readback and validation branches, then removes the link and deletes the catalog and markets.',
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
    captureId: 'price-list-fixed-prices-variant-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-price-list-fixed-prices-variant-lifecycle-conformance.mts',
    purpose:
      'Variant-level price-list fixed-price add, update, delete, and downstream PriceList.prices(originType: FIXED) read-after-write behavior.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}price-list-fixed-prices-variant-lifecycle.json`,
      'config/parity-specs/markets/price-list-fixed-prices-variant-lifecycle.json',
      'config/parity-requests/markets/price-list-fixed-prices-add.graphql',
      'config/parity-requests/markets/price-list-fixed-prices-by-product-read.graphql',
      'config/parity-requests/markets/price-list-fixed-prices-by-product-update-validation.graphql',
      'config/parity-requests/markets/price-list-fixed-prices-by-product-update.graphql',
      'config/parity-requests/markets/price-list-fixed-prices-delete.graphql',
      'config/parity-requests/markets/price-list-fixed-prices-read.graphql',
      'config/parity-requests/markets/price-list-fixed-prices-update.graphql',
    ],
    cleanupBehavior:
      'Deletes the target variant fixed price before and after recording the add/update/delete lifecycle.',
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
    captureId: 'catalog-context-update-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-catalog-context-update-conformance.ts',
    purpose:
      'catalogContextUpdate required-context validation, remove-only context updates, duplicate market add behavior, catalog-not-found typing, and downstream catalog reads.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}catalog-context-update-lifecycle.json`,
      'config/parity-specs/markets/catalog-context-update-no-args.json',
      'config/parity-specs/markets/catalog-context-update-removes-only.json',
      'config/parity-specs/markets/catalog-context-update-market-taken.json',
      'config/parity-specs/markets/catalog-context-update-catalog-not-found.json',
      'config/parity-requests/markets/catalog-context-update-catalog-create.graphql',
      'config/parity-requests/markets/catalog-context-update-catalog-not-found.graphql',
      'config/parity-requests/markets/catalog-context-update-market-create.graphql',
      'config/parity-requests/markets/catalog-context-update-market-taken.graphql',
      'config/parity-requests/markets/catalog-context-update-no-args.graphql',
      'config/parity-requests/markets/catalog-context-update-read.graphql',
      'config/parity-requests/markets/catalog-context-update-removes-only.graphql',
      'config/parity-requests/markets/catalog-context-update-unknown-id-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable markets and MarketCatalogs, records catalogContextUpdate branches, deletes catalogs in reverse creation order, then deletes markets in reverse creation order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'quantity-rules-add-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-quantity-rules-add-validation-conformance.mts',
    purpose:
      'quantityRulesAdd numeric bounds, minimum/maximum range, increment divisibility, and duplicate variant validation branches.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}quantity-rules-add-validation.json`,
      'config/parity-specs/markets/quantity-rules-add-validation.json',
      'config/parity-requests/markets/quantity-rules-add-validation.graphql',
    ],
    cleanupBehavior:
      'Uses existing conformance price-list and product-variant records; all captured quantityRulesAdd calls reject before staging quantity rules, so no cleanup mutation is required.',
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
    captureId: 'market-localization-money-metafield-remove',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-localization-money-metafield-remove-conformance.mts',
    purpose:
      'Definition-backed money metafield marketLocalizationsRegister/remove success, returned deleted rows, and read-after-remove behavior.',
    requiredAuthScopes: [
      'read_markets',
      'write_markets',
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-localization-money-metafield-remove-parity.json`,
      'config/parity-specs/markets/market-localization-money-metafield-remove.json',
      'config/parity-requests/markets/market-localization-money-metafield-read.graphql',
      'config/parity-requests/markets/market-localization-money-metafield-register.graphql',
      'config/parity-requests/markets/market-localization-money-metafield-remove.graphql',
    ],
    cleanupBehavior:
      'Creates a disposable product-owned money metafield definition and product metafield, registers localizations for two markets, removes each tuple, then deletes the product and definition.',
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
      'config/parity-specs/markets/web-presence-partial-update-alternate-locales.json',
    ],
    cleanupBehavior:
      'Creates one disposable subfolder web presence, updates it, deletes it, records one multi-locale disposable web presence with subfolder suffix intl, records one partial alternate-locale-only update, deletes all disposable web presences, and verifies the baseline read after cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'markets-delete-cascades',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-delete-cascades-conformance.mts',
    purpose:
      'marketDelete, catalogDelete, and priceListDelete downstream cascade behavior for web presences, catalog contexts, catalog/price-list detachment, and fixed price cleanup.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}delete-cascades-parity.json`,
      'config/parity-specs/markets/market-delete-cascades-web-presence.json',
      'config/parity-specs/markets/catalog-delete-detaches-price-list.json',
      'config/parity-specs/markets/price-list-delete-clears-fixed-prices.json',
      'config/parity-requests/markets/market-delete-cascade-delete.graphql',
      'config/parity-requests/markets/market-delete-cascade-read.graphql',
      'config/parity-requests/markets/market-delete-cascade-setup-read.graphql',
      'config/parity-requests/markets/catalog-delete-detaches-price-list-delete.graphql',
      'config/parity-requests/markets/catalog-delete-detaches-price-list-read.graphql',
      'config/parity-requests/markets/catalog-delete-detaches-price-list-setup-read.graphql',
      'config/parity-requests/markets/price-list-delete-clears-fixed-prices-delete.graphql',
      'config/parity-requests/markets/price-list-delete-clears-fixed-prices-read.graphql',
      'config/parity-requests/markets/price-list-delete-clears-fixed-prices-setup-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable web presence, markets, catalogs, price lists, and one fixed variant price; the live delete scenarios remove the targeted records, and cleanup deletes any surviving setup records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'marketing',
    captureId: 'marketing',
    scriptPath: 'scripts/capture-marketing-conformance.mts',
    purpose: 'Marketing activity/event/engagement roots and mutation branches.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/marketing/marketing-activity-create-external-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/marketing/marketing-activity-delete-external-guards.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/marketing/marketing-activity-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/marketing/marketing-activity-update-external-multi-selector.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/marketing/marketing-activity-upsert-immutable-fields.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/marketing/marketing-baseline-read.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/marketing/marketing-engagement-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/marketing/marketing-invalid-id-read.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/marketing/marketing-schema-inventory.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/marketing/marketing-engagement-currency-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/marketing/marketing-engagement-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/marketing/marketing-native-activity-validation.json',
    ],
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
      'config/parity-requests/marketing/marketing-activity-immutable-create.graphql',
      'config/parity-requests/marketing/marketing-activity-immutable-update.graphql',
      'config/parity-requests/marketing/marketing-activity-immutable-upsert.graphql',
    ],
    cleanupBehavior:
      'Creates disposable parent and child external marketing activities, captures rejected immutable-field updates, then deletes every disposable remote ID.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'marketing',
    captureId: 'marketing-activity-update-external-multi-selector',
    scriptPath: 'scripts/capture-marketing-activity-update-external-multi-selector-conformance.mts',
    purpose:
      'External marketing activity update selector conjunction semantics for conflicting remoteId and UTM matches.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-activity-update-external-multi-selector.json`,
      'config/parity-specs/marketing/marketing-activity-update-external-multi-selector.json',
      'config/parity-requests/marketing/marketing-activity-update-external-multi-selector-read.graphql',
      'config/parity-requests/marketing/marketing-activity-update-external-multi-selector.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable external marketing activities, captures a rejected conflicting-selector update, reads back the first activity unchanged, and deletes both remote IDs.',
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
    captureId: 'marketing-activity-create-external-validation',
    scriptPath: 'scripts/capture-marketing-activity-create-external-validation-conformance.mts',
    purpose:
      'External marketing activity create validation for unknown channel handles, budget/adSpend currency mismatch, and uniqueness userErrors.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-activity-create-external-validation.json`,
      'config/parity-specs/marketing/marketing-activity-create-external-validation.json',
      'config/parity-requests/marketing/marketing-activity-create-external-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable external marketing activities needed for uniqueness probes, captures rejected validation branches, then deletes every disposable remote ID.',
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
    purpose:
      'Segment query grammar support for broad segmentCreate/segmentUpdate save-time validation and `NOT CONTAINS` customer-tag predicates.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}segment-query-grammar-not-contains.json`,
      `${CAPTURE_ROOT}segment-create-update-query-grammar.json`,
      'config/parity-specs/segments/segment-create-update-query-grammar.json',
      'config/parity-requests/segments/segment-create-update-query-grammar-create.graphql',
      'config/parity-requests/segments/segment-create-update-query-grammar-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable segments, deletes them during cleanup, and leaves only Shopify async query state.',
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
      'config/parity-requests/segments/segment-create-validation-limits.graphql',
      'config/parity-requests/segments/segment-update-name-validation-limits.graphql',
      'config/parity-requests/segments/segment-update-query-validation-limits.graphql',
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
      'config/parity-requests/segments/segments-user-errors-shape-member-query-create.graphql',
      'config/parity-requests/segments/segments-user-errors-shape-segment-create.graphql',
      'config/parity-requests/segments/segments-user-errors-shape-segment-delete.graphql',
      'config/parity-requests/segments/segments-user-errors-shape-segment-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable segment for the segmentUpdate id-only validation branch and deletes it during cleanup; all other captured branches are validation-only.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'segments',
    captureId: 'segment-update-delete-malformed-gid',
    scriptPath: 'scripts/capture-segment-update-delete-malformed-gid-conformance.ts',
    purpose:
      'segmentUpdate/segmentDelete malformed, empty, wrong-resource, and unknown Segment id validation response envelopes.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}segment-update-delete-malformed-gid.json`,
      'config/parity-specs/segments/segment-update-delete-malformed-gid.json',
      'config/parity-requests/segments/segment-delete-malformed-gid.graphql',
      'config/parity-requests/segments/segment-update-malformed-gid.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no live segment setup or cleanup expected.',
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
      'config/parity-requests/segments/customer-segment-members-query-create-validation-and-shape.graphql',
      'config/parity-requests/segments/customer-segment-members-query-lookup-validation-and-shape.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable segment for the segmentId-backed branch and deletes it during cleanup; member-query jobs are async Shopify state without a cleanup mutation.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-content-lifecycle',
    scriptPath: 'scripts/capture-online-store-content-lifecycle-conformance.ts',
    purpose:
      'Online store blog, page, and article lifecycle success paths, downstream read-after-write behavior, empty reads, counts, and unknown-comment guardrails.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}online-store-content-lifecycle.json`,
      'config/parity-specs/online-store/online-store-content-lifecycle.json',
      'config/parity-requests/online-store/online-store-content-article-create.graphql',
      'config/parity-requests/online-store/online-store-content-comment-unknown.graphql',
      'config/parity-requests/online-store/online-store-content-create.graphql',
      'config/parity-requests/online-store/online-store-content-delete.graphql',
      'config/parity-requests/online-store/online-store-content-read-after-update.graphql',
      'config/parity-requests/online-store/online-store-content-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable blog, page, and article; records lifecycle reads/writes; deletes all created content during the scenario and retries cleanup for any remaining record.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-article-media-navigation-follow-through',
    scriptPath: 'scripts/capture-online-store-article-media-navigation-follow-through-conformance.ts',
    purpose:
      'Article image/metafield create/update/read behavior, Page/Article schema boundaries, and page-backed menu navigation follow-through evidence.',
    requiredAuthScopes: [
      'read_content',
      'write_content',
      'read_online_store_navigation',
      'write_online_store_navigation',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}online-store-article-media-navigation-follow-through.json`,
      'config/parity-specs/online-store/online-store-article-media-navigation-follow-through.json',
      'config/parity-requests/online-store/online-store-article-media.graphql',
    ],
    cleanupBehavior:
      'Creates disposable blog, article, page, and menu records; deletes the menu during the scenario, then deletes remaining article/page/blog records in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Menu evidence is captured for future navigation modeling; the executable parity target covers the locally supported article image/metafield create payload.',
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
      'config/parity-requests/online-store/online-store-article-create-validation-article-create.graphql',
      'config/parity-requests/online-store/online-store-article-create-validation-blog-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable blog for blogId-backed branches, deletes the success-path article, then deletes the blog.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-article-update-validation',
    scriptPath: 'scripts/capture-online-store-article-update-validation-conformance.ts',
    purpose: 'articleUpdate ambiguous author, author user existence, and image URL validation branches.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}online-store-article-update-validation.json`,
      'config/parity-specs/online-store/article_update_validation.json',
      'config/parity-requests/online-store/online-store-article-update-validation-article-create.graphql',
      'config/parity-requests/online-store/online-store-article-update-validation-article-update.graphql',
      'config/parity-requests/online-store/online-store-article-update-validation-blog-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable blog and article setup record; invalid articleUpdate attempts should not mutate, and cleanup deletes the article then blog.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-delete-cascades',
    scriptPath: 'scripts/capture-online-store-delete-cascade-conformance.ts',
    purpose:
      'blogDelete and articleDelete dependent-destroy behavior for child articles and comments, including downstream null/empty reads.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}article-delete-cascades-comments.json`,
      `${CAPTURE_ROOT}blog-delete-cascades-articles-and-comments.json`,
      'config/parity-specs/online-store/article_delete_cascades_comments.json',
      'config/parity-specs/online-store/blog_delete_cascades_articles_and_comments.json',
      'config/parity-requests/online-store/article-delete-cascades-comments-read.graphql',
      'config/parity-requests/online-store/article-delete-cascades-comments.graphql',
      'config/parity-requests/online-store/blog-delete-cascades-articles-and-comments-read.graphql',
      'config/parity-requests/online-store/blog-delete-cascades-articles-and-comments.graphql',
    ],
    cleanupBehavior:
      'Creates disposable blogs/articles and REST article comments, then deletes the article or blog during the scenario; failure cleanup deletes any remaining article/blog records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-comment-delete-true-destroy',
    scriptPath: 'scripts/capture-online-store-comment-delete-true-destroy-conformance.ts',
    purpose:
      'commentDelete true-destroy behavior for singular comment reads, root/nested comment connections, and Article.commentsCount.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}comment-delete-true-destroy.json`,
      'config/parity-specs/online-store/comment-delete-true-destroy.json',
      'config/parity-requests/online-store/comment-delete-true-destroy-approve.graphql',
      'config/parity-requests/online-store/comment-delete-true-destroy-delete.graphql',
      'config/parity-requests/online-store/comment-delete-true-destroy-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable blog/article and one REST article comment, approves and deletes the comment during the scenario, then deletes the article and blog in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-comment-moderation-state-transitions',
    scriptPath: 'scripts/capture-online-store-comment-moderation-state-transitions-conformance.ts',
    purpose:
      'commentApprove, commentSpam, and commentNotSpam state-machine preconditions, idempotent branches, and invalid source-state userErrors.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}comment-moderation-state-transitions.json`,
      'config/parity-specs/online-store/comment-moderation-state-transitions.json',
      'config/parity-requests/online-store/comment-moderation-state-transition-approve.graphql',
      'config/parity-requests/online-store/comment-moderation-state-transition-not-spam.graphql',
      'config/parity-requests/online-store/comment-moderation-state-transition-spam.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable moderated blog/article and REST article comments, prepares PUBLISHED and SPAM source states with Admin GraphQL moderation roots, then deletes the article and blog during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-theme-update-role-not-an-input',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-online-store-theme-update-validation-conformance.ts',
    purpose: 'themeUpdate rejects fields that are not exposed by OnlineStoreThemeInput before resolver execution.',
    requiredAuthScopes: ['authenticated_admin_graphql'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}theme-update-role-not-an-input.json`,
      'config/parity-specs/online-store/theme_update_role_not_an_input.json',
      'config/parity-requests/online-store/theme-update-role-not-an-input.graphql',
    ],
    cleanupBehavior:
      'Validation-only schema capture; the dummy theme ID is never resolved and no live theme state is modified.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The current conformance app is blocked from themeUpdate resolver writes by Shopify write_themes exemption requirements, so blank-name, valid-rename, and locked-theme resolver branches are covered by executable local-runtime parity.',
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
      'config/parity-requests/online-store/online-store-body-script-article-create.graphql',
      'config/parity-requests/online-store/online-store-body-script-article-read.graphql',
      'config/parity-requests/online-store/online-store-body-script-blog-create.graphql',
      'config/parity-requests/online-store/online-store-body-script-page-create.graphql',
      'config/parity-requests/online-store/online-store-body-script-page-read.graphql',
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
      'config/parity-requests/online-store/online-store-body-script-article-create.graphql',
      'config/parity-requests/online-store/online-store-body-script-article-read.graphql',
      'config/parity-requests/online-store/online-store-body-script-blog-create.graphql',
      'config/parity-requests/online-store/online-store-body-script-page-create.graphql',
      'config/parity-requests/online-store/online-store-body-script-page-read.graphql',
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
      'config/parity-requests/online-store/online-store-content-required-fields-article-create.graphql',
      'config/parity-requests/online-store/online-store-content-required-fields-blog-create.graphql',
      'config/parity-requests/online-store/online-store-content-required-fields-page-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable blog for articleCreate blogId-backed validation, then deletes it during cleanup. Blank-title page/blog/article attempts do not create records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'online-store',
    captureId: 'online-store-invalid-publish-date',
    scriptPath: 'scripts/capture-online-store-invalid-publish-date-conformance.ts',
    purpose:
      'pageCreate, articleCreate, pageUpdate, and articleUpdate validation for publishing content with a future publishDate, plus scheduled-publish allowed setup branches.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}online-store-invalid-publish-date.json`,
      'config/parity-specs/online-store/page_create_invalid_publish_date.json',
      'config/parity-specs/online-store/article_create_invalid_publish_date.json',
      'config/parity-specs/online-store/page_update_invalid_publish_date.json',
      'config/parity-specs/online-store/article_update_invalid_publish_date.json',
      'config/parity-requests/online-store/online-store-invalid-publish-date-article-create.graphql',
      'config/parity-requests/online-store/online-store-invalid-publish-date-article-update.graphql',
      'config/parity-requests/online-store/online-store-invalid-publish-date-blog-create.graphql',
      'config/parity-requests/online-store/online-store-invalid-publish-date-page-create.graphql',
      'config/parity-requests/online-store/online-store-invalid-publish-date-page-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable blog plus unpublished scheduled page/article setup records; invalid publish attempts do not create records, and cleanup deletes the scheduled article, scheduled page, and blog.',
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
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/collection-create-and-add-products-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/collection-create-initial-products-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/collection-product-membership-job-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-add-products-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-create-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-delete-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-detail.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-remove-products-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-reorder-products-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collection-update-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/store-properties/collection-publication-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/collections-catalog.json',
    ],
    cleanupBehavior: 'Read-only capture against existing store collections; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    captureId: 'collection-mutations',
    scriptPath: 'scripts/capture-collection-mutation-conformance.mts',
    purpose: 'collectionCreate/update/delete/addProducts/removeProducts mutation family.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}collection-create-parity.json`,
      `${CAPTURE_ROOT}collection-publication-parity.json`,
      `${CAPTURE_ROOT}collection-add-products-parity.json`,
      `${CAPTURE_ROOT}collection-reorder-products-parity.json`,
      `${CAPTURE_ROOT}collection-update-parity.json`,
      `${CAPTURE_ROOT}collection-remove-products-parity.json`,
      `${CAPTURE_ROOT}collection-delete-parity.json`,
    ],
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
      'config/parity-requests/products/collectionCreate-and-add-products-add.graphql',
      'config/parity-requests/products/collectionCreate-and-add-products-count-read.graphql',
      'config/parity-requests/products/collectionCreate-and-add-products-create.graphql',
      'config/parity-requests/products/collectionCreate-and-add-products-remove.graphql',
    ],
    cleanupBehavior:
      'Creates disposable reserved-like, smart, and custom collections and deletes every successful collectionCreate result during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    captureId: 'collection-product-membership-job-parity',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-collection-product-membership-job-conformance.mts',
    purpose:
      'collectionAddProductsV2 and collectionRemoveProducts smart collection guards, async Job payload/readback, unknown productIds acceptance, and productIds cap validation.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}collection-product-membership-job-parity.json`,
      'config/parity-specs/products/collection-product-membership-job-parity.json',
      'config/parity-requests/products/collection-product-membership-job-add-v2.graphql',
      'config/parity-requests/products/collection-product-membership-job-create.graphql',
      'config/parity-requests/products/collection-product-membership-job-read.graphql',
      'config/parity-requests/products/collection-product-membership-job-remove.graphql',
    ],
    cleanupBehavior:
      'Creates disposable smart and custom collections, records validation/job branches, and deletes both collections during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    captureId: 'collection-publications',
    scriptPath: 'scripts/capture-collection-mutation-conformance.mts',
    purpose: 'Collection publication behavior covered by the collection mutation harness when enabled.',
    requiredAuthScopes: ['read_products', 'write_products', 'publication/channel access for the app'],
    fixtureOutputs: [`${CAPTURE_ROOT}collection-publication-parity.json`],
    cleanupBehavior: 'Shares disposable collection cleanup with the collection mutation harness.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'store-properties',
    captureId: 'locations',
    scriptPath: 'scripts/capture-location-conformance.mts',
    purpose: 'Location roots and inventory/publication-adjacent store property reads.',
    requiredAuthScopes: ['read_locations'],
    fixtureOutputs: [
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/locations-catalog.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/store-properties/locations-catalog.json',
      `${CAPTURE_ROOT}location-custom-id-miss.json`,
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/store-properties/business-entities-catalog.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/store-properties/business-entities-catalog.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/store-properties/business-entity-fallbacks.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/store-properties/business-entity-fallbacks.json',
    ],
    cleanupBehavior: 'Read-only by default; location lifecycle writes need disposable location setup and cleanup.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'store-properties',
    captureId: 'store-properties',
    scriptPath: 'scripts/capture-location-conformance.mts',
    purpose: 'Store property roots sharing the location capture harness.',
    requiredAuthScopes: ['read_locations', 'read_products'],
    fixtureOutputs: [
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/store-properties/store-properties-baseline.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/store-properties/store-properties-baseline.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/locations-catalog.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/store-properties/locations-catalog.json',
      `${CAPTURE_ROOT}location-custom-id-miss.json`,
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/store-properties/business-entities-catalog.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/store-properties/business-entities-catalog.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/store-properties/business-entity-fallbacks.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/store-properties/business-entity-fallbacks.json',
    ],
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
      'config/parity-requests/store-properties/location-edit-fields-and-state-machine-read.graphql',
      'config/parity-requests/store-properties/location-edit-fields-and-state-machine.graphql',
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
    captureId: 'location-deactivate-state-machine',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-location-deactivate-state-machine-conformance.mts',
    purpose:
      'locationDeactivate destination-location validation and source deactivation state-machine guards for same destination, inactive destination, active inventory, only-online fulfillment, and permanent deactivation block.',
    requiredAuthScopes: ['read_locations', 'write_locations', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-deactivate-state-machine.json`,
      'config/parity-specs/store-properties/location-deactivate-state-machine.json',
      'config/parity-requests/store-properties/location-deactivate-state-machine-with-destination.graphql',
      'config/parity-requests/store-properties/location-deactivate-state-machine.graphql',
    ],
    cleanupBehavior:
      'Creates disposable merchant-managed locations and a temporary product/inventory level, temporarily disables/restores online fulfillment on pre-existing locations for the only-online branch, then deactivates/deletes disposable locations.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    captureId: 'shop-policies',
    scriptPath: 'scripts/capture-shop-policy-conformance.ts',
    purpose: 'shopPolicyUpdate and legal-policy read/write behavior.',
    requiredAuthScopes: ['read_content', 'write_content or policy-management access'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/store-properties/shop-policy-update-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/store-properties/shop-policy-update-title-url-and-body-rendering.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/store-properties/shop-policy-update-parity.json',
      'config/parity-specs/store-properties/shop-policy-update-title-url-and-body-rendering.json',
    ],
    cleanupBehavior:
      'Restores prior policy content when a write branch is captured. Newly created policy rows may remain on shops where Shopify does not expose deletion, but their bodies are reset to the prior empty fallback.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'store-properties',
    captureId: 'shop-policy-subscription-blank-body',
    scriptPath: 'scripts/capture-shop-policy-subscription-blank-body-conformance.ts',
    purpose:
      'shopPolicyUpdate SUBSCRIPTION_POLICY blank and whitespace body validation plus downstream shopPolicies non-presence.',
    requiredAuthScopes: ['read_content', 'write_content or policy-management access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}shop-policy-update-subscription-blank-body.json`,
      'config/parity-specs/store-properties/shop-policy-update-subscription-blank-body.json',
      'config/parity-requests/store-properties/shopPolicyUpdate-subscription-blank-body.graphql',
      'config/parity-requests/store-properties/shopPolicyUpdate-subscription-blank-body-downstream-read.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture. Rejected subscription-policy writes must not mutate policy content or create a blank downstream policy.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'privacy',
    captureId: 'privacy',
    scriptPath: 'scripts/capture-privacy-conformance.ts',
    purpose: 'Privacy/data-sale read and mutation roots.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'privacy API access'],
    fixtureOutputs: [`${CAPTURE_ROOT}privacy-conformance.json`],
    cleanupBehavior: 'Uses disposable customer records where writes are captured.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'privacy',
    captureId: 'data-sale-opt-out',
    scriptPath: 'scripts/capture-data-sale-opt-out-conformance.ts',
    purpose: 'dataSaleOptOut behavior and downstream customer privacy read effects.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'privacy API access'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/privacy/data-sale-opt-out-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/privacy/data-sale-opt-out-whitespace-email.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/privacy/data-sale-opt-out-new-customer-defaults.json',
    ],
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
      `${CAPTURE_ROOT}admin-graphql-root-operation-introspection.json`,
      'src/shopify_draft_proxy/proxy/operation_registry_data.gleam',
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
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/order-create-validation-matrix-extended.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/order-create-validation-matrix.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/order-edit-residual-calculated-edits.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/order-edit-residual-live-capture.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/order-edit-commit-history-and-fulfillment-orders.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/order-edit-lifecycle-user-errors.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/payments/order-capture-validation.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-create-inline-missing-order.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-create-inline-null-order.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-create-missing-order.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-create-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-edit-add-variant-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-edit-begin-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-edit-commit-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-edit-set-quantity-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-empty-state.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-update-inline-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-update-inline-null-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-update-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-update-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/order-update-unknown-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-catalog-count-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-happy-path.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-validation.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-edit-existing-order-zero-removal.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-merchant-detail-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-orders-catalog.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-orders-count.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-orders-invalid-email-query.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/fulfillment-cancel-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/fulfillment-create-preconditions.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/fulfillment-cancel-inline-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/fulfillment-cancel-inline-null-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/fulfillment-cancel-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/fulfillment-cancel-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/fulfillment-create-invalid-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/fulfillment-tracking-info-update-inline-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/fulfillment-tracking-info-update-inline-null-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/fulfillment-tracking-info-update-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/fulfillment-tracking-info-update-parity.json',
    ],
    cleanupBehavior: 'Creates/cancels disposable orders only after credential and store-state probes pass.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'orders',
    captureId: 'abandoned-checkout-empty-read',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-abandoned-checkout-conformance.ts',
    purpose:
      'Abandoned checkout empty/no-data reads and unknown abandonment delivery-status validation against the disposable conformance store.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}abandoned-checkout-empty-read.json`,
      `${CAPTURE_ROOT}abandonment-delivery-status-unknown.json`,
      'config/parity-specs/orders/abandoned-checkout-empty-read.json',
      'config/parity-specs/orders/abandonment-delivery-status-unknown.json',
      'config/parity-requests/orders/abandoned-checkout-empty-read.graphql',
      'config/parity-requests/orders/abandoned-checkout-empty-read.variables.json',
      'config/parity-requests/orders/abandonment-delivery-status-unknown.graphql',
      'config/parity-requests/orders/abandonment-delivery-status-unknown.variables.json',
    ],
    cleanupBehavior: 'Read/validation-only capture; unknown abandonment IDs do not mutate Shopify state.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-management-mutations',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-management-conformance.ts',
    purpose:
      'orderClose, orderOpen, orderCancel, orderCustomerSet/Remove, orderInvoiceSend, orderCreateManualPayment access-denied, and taxSummaryCreate access-denied parity slices.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderCancel-parity.json`,
      `${CAPTURE_ROOT}orderClose-parity.json`,
      `${CAPTURE_ROOT}orderCreateManualPayment-access-denied-parity.json`,
      `${CAPTURE_ROOT}orderCustomerRemove-parity.json`,
      `${CAPTURE_ROOT}orderCustomerSet-parity.json`,
      `${CAPTURE_ROOT}orderInvoiceSend-parity.json`,
      `${CAPTURE_ROOT}orderOpen-parity.json`,
      `${CAPTURE_ROOT}taxSummaryCreate-access-denied-parity.json`,
      `${CAPTURE_ROOT}order-management-cleanup.json`,
      'config/parity-specs/orders/orderCancel-parity.json',
      'config/parity-specs/orders/orderClose-parity.json',
      'config/parity-specs/orders/orderCreateManualPayment-access-denied-parity.json',
      'config/parity-specs/orders/orderCustomerRemove-parity.json',
      'config/parity-specs/orders/orderCustomerSet-parity.json',
      'config/parity-specs/orders/orderInvoiceSend-parity.json',
      'config/parity-specs/orders/orderOpen-parity.json',
      'config/parity-specs/orders/taxSummaryCreate-access-denied-parity.json',
      'config/parity-requests/orders/orderCancel-parity.graphql',
      'config/parity-requests/orders/orderClose-parity.graphql',
      'config/parity-requests/orders/orderCreateManualPayment-access-denied-parity.graphql',
      'config/parity-requests/orders/orderCustomerRemove-parity.graphql',
      'config/parity-requests/orders/orderCustomerSet-parity.graphql',
      'config/parity-requests/orders/orderInvoiceSend-parity.graphql',
      'config/parity-requests/orders/orderOpen-parity.graphql',
      'config/parity-requests/orders/order-management-downstream-read.graphql',
      'config/parity-requests/orders/taxSummaryCreate-access-denied-parity.graphql',
    ],
    cleanupBehavior:
      'Creates disposable test orders and a disposable customer, records the mutation slices, cancels created orders, and deletes the customer when Shopify permits cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-edit-commit-history-fulfillment',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-edit-commit-history-fulfillment-conformance.ts',
    purpose:
      'orderEditCommit downstream edit-history, fulfillment-order remaining quantity, and current totals/tax-line behavior after a quantity decrement plus variant addition.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}order-edit-commit-history-and-fulfillment-orders.json`,
      'config/parity-specs/orders/orderEditCommit-history-and-fulfillment-orders.json',
      'config/parity-requests/orders/orderEditCommit-history-fulfillment-addVariant.graphql',
      'config/parity-requests/orders/orderEditCommit-history-fulfillment-begin.graphql',
      'config/parity-requests/orders/orderEditCommit-history-fulfillment-commit.graphql',
      'config/parity-requests/orders/orderEditCommit-history-fulfillment-downstream-read.graphql',
      'config/parity-requests/orders/orderEditCommit-history-fulfillment-setQuantity.graphql',
    ],
    cleanupBehavior: 'Creates one disposable test order, commits one order edit, then cancels the order with restock.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
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
    captureId: 'order-update-input-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-update-input-validation-conformance.ts',
    purpose:
      'orderUpdate empty-input, malformed phone, malformed shipping address, and happy-path note update validation parity.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderUpdate-input-validation.json`,
      'config/parity-specs/orders/orderUpdate-input-validation.json',
      'config/parity-requests/orders/orderUpdate-input-validation.graphql',
    ],
    cleanupBehavior: 'Creates a disposable paid test order and cancels it after capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-invoice-send-email-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-invoice-send-email-validation-conformance.ts',
    purpose:
      'orderInvoiceSend resolved-recipient and explicit EmailInput.to validation, plus order-email happy-path baseline.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderInvoiceSend-email-validation.json`,
      'config/parity-specs/orders/orderInvoiceSend-email-validation.json',
      'config/parity-requests/orders/orderInvoiceSend-email-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable test orders, records validation and happy-path invoice-send behavior, then cancels created orders in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'fulfillment-create-preconditions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-create-preconditions-conformance.ts',
    purpose:
      'fulfillmentCreate cancelled/closed fulfillment order, over-quantity, in-progress, and happy-path public Admin API behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-create-preconditions.json`,
      'config/parity-specs/orders/fulfillmentCreate-preconditions.json',
      'config/parity-requests/orders/fulfillmentCreate-preconditions.graphql',
    ],
    cleanupBehavior:
      'Creates disposable test orders, cancels/deletes where possible, and deletes fulfilled orders after capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Public fulfillmentCreate userErrors expose field/message only; Admin 2026-04 accepts fulfillmentCreate after fulfillmentOrderReportProgress leaves the fulfillment order IN_PROGRESS.',
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
      'config/parity-requests/orders/orderEdit-lifecycle-userErrors-addVariant.graphql',
      'config/parity-requests/orders/orderEdit-lifecycle-userErrors-begin.graphql',
      'config/parity-requests/orders/orderEdit-lifecycle-userErrors-commit.graphql',
      'config/parity-requests/orders/orderEdit-lifecycle-userErrors-setQuantity.graphql',
    ],
    cleanupBehavior: 'Validation-only order-edit probes use missing Shopify GIDs and do not create merchant resources.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-edit-add-custom-item-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-edit-add-custom-item-validation-conformance.ts',
    purpose:
      'orderEditAddCustomItem title, quantity, price, currency, and happy-path custom-item validation against a disposable order-edit session.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderEditAddCustomItem-validation.json`,
      'config/parity-specs/orders/orderEditAddCustomItem-validation.json',
      'config/parity-requests/orders/orderEditAddCustomItem-validation-begin.graphql',
      'config/parity-requests/orders/orderEditAddCustomItem-validation-case.graphql',
      'config/parity-requests/orders/orderEditAddCustomItem-validation-inline-missing-currency.graphql',
      'config/parity-requests/orders/orderEditAddCustomItem-validation-missing-title.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable CAD test order, begins an order edit, records validation and happy-path branches, then cancels the order with restock.',
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
    purpose: 'Draft order create/update/delete/complete, duplicate lifecycle reset, and downstream read behavior.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders', 'read_products'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/draft-order-by-id-not-found.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/draft-order-create-from-order-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/draft-order-delete-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/draft-order-detail.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/draft-order-duplicate-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/draft-order-duplicate-resets-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/draft-order-residual-helper-roots.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/draft-order-update-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/draft-order-create-validation-matrix.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/draft-order-invoice-send-safety.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-complete-inline-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-complete-inline-null-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-complete-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-complete-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-create-from-order-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-create-inline-missing-input.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-create-inline-null-input.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-create-missing-input.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-create-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-delete-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-detail.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-duplicate-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-update-inline-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-update-inline-null-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-update-missing-id.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-order-update-parity.json',
    ],
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
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-app-function-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-automatic-basic-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-automatic-basic-nodes-read.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-basic-disallowed-discount-on-quantity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-bulk-selector-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-buyer-context-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-bxgy-disallowed-value-shapes.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-bxgy-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-class-inference.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-code-basic-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-code-required-blank-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-combines-with-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-context-customer-selection-conflict.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-customer-gets-value-multiple-types.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-customer-selection-internal-conflicts.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-delete-unknown-id.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-free-shipping-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-invalid-date-range-all-types.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-minimum-requirement-exclusivity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-redeem-code-bulk-add-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-redeem-code-bulk-delete-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-redeem-code-bulk.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-status-time-window-derivation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-timestamps-monotonic.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-update-edge-cases.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-validation-branches.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-value-bounds.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-automatic-basic-detail-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-catalog-code-filter-empty-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-catalog-empty-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-catalog-non-empty-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-catalog-status-filter-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-code-basic-detail-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-delete-cleanup.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-nodes-catalog.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-nodes-count.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-scope-probe.json',
    ],
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
    captureId: 'discount-basic-disallowed-discount-on-quantity',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-basic-disallowed-discount-on-quantity-conformance.ts',
    purpose: 'Basic code and automatic discount rejection of customerGets.value.discountOnQuantity on create/update.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-basic-disallowed-discount-on-quantity.json`,
      'config/parity-specs/discounts/discount-basic-disallowed-discount-on-quantity.json',
      'config/parity-requests/discounts/discount-basic-disallowed-discount-on-quantity-automatic-create.graphql',
      'config/parity-requests/discounts/discount-basic-disallowed-discount-on-quantity-automatic-update.graphql',
      'config/parity-requests/discounts/discount-basic-disallowed-discount-on-quantity-code-create.graphql',
      'config/parity-requests/discounts/discount-basic-disallowed-discount-on-quantity-code-update.graphql',
    ],
    cleanupBehavior: 'Creates one disposable basic code discount and one basic automatic discount, then deletes both.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-delete-unknown-id',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-delete-unknown-id-conformance.ts',
    purpose:
      'discountCodeDelete and discountAutomaticDelete unknown-id INVALID userErrors plus successful delete regression for setup discounts.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-delete-unknown-id.json`,
      'config/parity-specs/discounts/discount-delete-unknown-id.json',
      'config/parity-requests/discounts/discount-delete-unknown-id-automatic.graphql',
      'config/parity-requests/discounts/discount-delete-unknown-id-code.graphql',
      'config/parity-requests/discounts/discount-delete-unknown-id-setup.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable basic code discount and one disposable basic automatic discount, then deletes both during the scenario with finally-block cleanup on failure.',
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
      'config/parity-requests/discounts/discount-class-inference-create.graphql',
      'config/parity-requests/discounts/discount-class-inference-read.graphql',
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
      'config/parity-requests/discounts/discount-redeem-code-bulk-validation-add.graphql',
      'config/parity-requests/discounts/discount-redeem-code-bulk-validation-create.graphql',
      'config/parity-requests/discounts/discount-redeem-code-bulk-validation-creation-read.graphql',
      'config/parity-requests/discounts/discount-redeem-code-bulk-validation-read.graphql',
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
      'config/parity-requests/discounts/discount-update-edge-cases-basic-create.graphql',
      'config/parity-requests/discounts/discount-update-edge-cases-basic-update.graphql',
      'config/parity-requests/discounts/discount-update-edge-cases-bulk-add.graphql',
      'config/parity-requests/discounts/discount-update-edge-cases-bxgy-create.graphql',
      'config/parity-requests/discounts/discount-update-edge-cases-unknown-update.graphql',
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
      'config/parity-requests/discounts/discount-status-time-window-derivation-create.graphql',
      'config/parity-requests/discounts/discount-status-time-window-derivation-read.graphql',
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
      'config/parity-requests/discounts/discount-customer-gets-value-multiple-types-create.graphql',
      'config/parity-requests/discounts/discount-customer-gets-value-multiple-types-update.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no discounts are created on successful capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-customer-selection-internal-conflicts',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-customer-selection-internal-conflicts-conformance.ts',
    purpose:
      'Discount customerSelection all/customers and all/customerSegments BadRequest parity plus public-schema saved-search coercion for basic code create inputs.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-customer-selection-internal-conflicts.json`,
      'config/parity-specs/discounts/discount-customer-selection-internal-conflicts.json',
      'config/parity-requests/discounts/discount-customer-selection-internal-conflicts-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable customer, one disposable customer segment, and one valid disposable code discount for the happy path; deletes all created resources after capture.',
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
      'config/parity-requests/discounts/discount-invalid-date-range-automatic-bxgy.graphql',
      'config/parity-requests/discounts/discount-invalid-date-range-automatic-free-shipping.graphql',
      'config/parity-requests/discounts/discount-invalid-date-range-code-basic.graphql',
      'config/parity-requests/discounts/discount-invalid-date-range-code-bxgy.graphql',
      'config/parity-requests/discounts/discount-invalid-date-range-code-free-shipping.graphql',
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
    captureId: 'functions-cart-transform-api-mismatch',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-cart-transform-api-mismatch-conformance.ts',
    purpose:
      'cartTransformCreate API-mismatched Function identifier userError code split for functionId versus functionHandle plus downstream empty cartTransforms read.',
    requiredAuthScopes: [
      'read_cart_transforms',
      'write_cart_transforms for cleanup of pre-existing conformance cart transforms',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-cart-transform-create-api-mismatch-by-identifier.json`,
      'config/parity-specs/functions/functions-cart-transform-create-api-mismatch-by-identifier.json',
      'config/parity-requests/functions/functions-cart-transform-create-api-mismatch-by-id.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-api-mismatch-by-handle.graphql',
    ],
    cleanupBehavior:
      'Deletes pre-existing cartTransforms before capturing validation Function mismatch probes, then verifies the failed probes leave cartTransforms empty.',
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
    captureId: 'functions-validation-create-error-shape',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-validation-create-error-shape-conformance.ts',
    purpose:
      'validationCreate unknown Function id, wrong Function API, missing Function identifier, and multiple Function identifier userError shapes.',
    requiredAuthScopes: [
      'read_validations',
      'write_validations for validationCreate userError capture',
      'released conformance-cart-transform Function in the installed conformance app',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-validation-create-error-shape.json`,
      'config/parity-specs/functions/functions-validation-create-error-shape.json',
    ],
    cleanupBehavior:
      'Captures validationCreate userErrors only; all branches return validation null and no live resources are created.',
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
    captureId: 'transaction-void-codes',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-transaction-void-codes-conformance.ts',
    purpose:
      'transactionVoid TRANSACTION_NOT_FOUND, AUTH_NOT_SUCCESSFUL, and AUTH_NOT_VOIDABLE public userError code shapes.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}transaction-void-codes.json`,
      'config/parity-specs/payments/transaction_void_codes.json',
      'config/parity-requests/payments/transaction-void-codes-order-capture.graphql',
      'config/parity-requests/payments/transaction-void-codes-order-create.graphql',
      'config/parity-requests/payments/transaction-void-codes-transaction-void.graphql',
    ],
    cleanupBehavior:
      'Creates disposable orders with capture and authorization transactions, captures void validation branches, captures one orderCapture setup, then cancels the disposable orders.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'order-capture-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-capture-validation-conformance.ts',
    purpose:
      'orderCapture multi-currency currency validation, missing parent transaction, invalid amount, over-capture, public manual-gateway finalCapture rejection, and follow-up capture behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}order-capture-validation.json`,
      'config/parity-specs/payments/order_capture_validation.json',
      'config/parity-requests/payments/order-capture-validation-order-capture.graphql',
      'config/parity-requests/payments/order-capture-validation-order-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable multi-currency authorization order, records validation and capture branches, then cancels the order during cleanup.',
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
    captureId: 'payment-terms-create-order-eligibility',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-terms-order-eligibility-conformance.ts',
    purpose: 'paymentTermsCreate Order eligibility rejection for paid, closed, and cancelled disposable Orders.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-terms-create-order-eligibility.json`,
      `${CAPTURE_ROOT}payment-terms-create-order-eligibility-cleanup.json`,
      'config/parity-specs/payments/payment-terms-create-order-eligibility.json',
      'config/parity-requests/payments/payment-terms-create-order-eligibility.graphql',
    ],
    cleanupBehavior:
      'Creates disposable paid, closed, and cancelled test Orders, captures paymentTermsCreate eligibility payloads, deletes closed-order payment terms when Shopify accepts them, and best-effort cancels created Orders afterward.',
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
      'config/parity-requests/payments/payment-customization-metafields-create.graphql',
      'config/parity-requests/payments/payment-customization-metafields-read.graphql',
      'config/parity-requests/payments/payment-customization-metafields-update.graphql',
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
      'config/parity-requests/payments/payment-customization-immutable-create.graphql',
      'config/parity-requests/payments/payment-customization-immutable-read.graphql',
      'config/parity-requests/payments/payment-customization-immutable-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable payment customization, captures rejected functionId replacement and readback behavior, then deletes the payment customization.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'payment-customization-create-validation-gaps',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-customization-create-validation-gaps-conformance.ts',
    purpose:
      'paymentCustomizationCreate required-metafields probe, Function identifier arbitration, missing identifier, and active customization limit probe.',
    requiredAuthScopes: ['read_payment_customizations', 'write_payment_customizations', 'shopifyFunctions read access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-customization-create-validation-gaps.json`,
      'config/parity-specs/payments/payment-customization-create-validation-gaps.json',
      'config/parity-requests/payments/payment-customization-create-validation-gaps.graphql',
    ],
    cleanupBehavior:
      'Deletes active payment customizations before capture, creates disposable active customizations in one validation request, then deletes every row returned by the request.',
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
      'config/parity-requests/admin-platform/admin-platform-backup-region-update-validation-missing-country-code.graphql',
      'config/parity-requests/admin-platform/admin-platform-backup-region-update-validation-null-country-code.graphql',
      'config/parity-requests/admin-platform/admin-platform-backup-region-update-validation-numeric-country-code.graphql',
      'config/parity-requests/admin-platform/admin-platform-backup-region-update-validation-typename.graphql',
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
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/refund-create-full-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/refund-create-over-refund-user-errors.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/refund-create-partial-shipping-restock-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/refund-create-user-errors-and-quantities.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/refund-create-full-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/refund-create-over-refund-user-errors.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/refund-create-partial-shipping-restock-parity.json',
    ],
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
      `${CAPTURE_ROOT}orderMarkAsPaid-state-and-money-cleanup.json`,
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
    domain: 'orders',
    captureId: 'return-reverse-logistics-introspection',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-return-reverse-logistics-introspection-conformance.ts',
    purpose:
      'Read-only Admin schema introspection for return and reverse-logistics roots used by local-runtime return parity specs.',
    requiredAuthScopes: ['schema introspection access through the active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}return-reverse-logistics-introspection.json`,
      'config/parity-specs/orders/return-lifecycle-local-staging.json',
      'config/parity-specs/orders/return-reverse-logistics-local-staging.json',
      'config/parity-specs/orders/return-request-decline-local-staging.json',
      'config/parity-specs/orders/removeFromReturn-local-staging.json',
      'config/parity-specs/orders/returnApprove-decline-state-preconditions.json',
    ],
    cleanupBehavior: 'Read-only introspection; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'return-status-preconditions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-return-status-preconditions-conformance.mts',
    purpose:
      'returnClose, returnReopen, and returnCancel status-machine preconditions, idempotent no-op branches, and processed-return cancel rejection.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_returns', 'write_returns', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}returnClose-Reopen-Cancel-state-preconditions.json`,
      'config/parity-specs/orders/returnClose-Reopen-Cancel-state-preconditions.json',
      'config/parity-requests/orders/return-cancel-state-precondition.graphql',
      'config/parity-requests/orders/return-close-state-precondition.graphql',
      'config/parity-requests/orders/return-reopen-state-precondition.graphql',
    ],
    cleanupBehavior:
      'Creates and fulfills disposable orders for requested, open/closed, cancelable, declined, and processed return states, records status precondition behavior, then cancels the orders.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-order-request-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-order-request-lifecycle-conformance.ts',
    purpose:
      'fulfillment-order fulfillment-request and cancellation-request lifecycle behavior using disposable orders and a temporary API fulfillment service.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-order-request-lifecycle.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-order-request-lifecycle.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-submit-request-lifecycle.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-accept-request-lifecycle.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-submit-cancellation-request-lifecycle.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-accept-cancellation-request-lifecycle.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-reject-request-lifecycle.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-reject-cancellation-request-lifecycle.graphql',
    ],
    cleanupBehavior:
      'Creates disposable orders and a temporary API fulfillment service, records fulfillment request/cancellation transitions, cancels orders where Shopify permits, and deletes the fulfillment service.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'orphaned-shipping-fulfillment-fixtures',
    scriptPath: 'scripts/capture-orphaned-shipping-fulfillment-fixtures-conformance.ts',
    purpose:
      'Re-records cassette-backed parity evidence for restored shipping and fulfillment fixture files that are consumed by standard parity specs.',
    requiredAuthScopes: [
      'read_shipping',
      'write_shipping',
      'read_orders',
      'write_orders',
      'read_fulfillments',
      'write_fulfillments',
    ],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/shipping-fulfillments/delivery-customization-promise-settings-blockers.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/carrier-service-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-order-request-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-service-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/fulfillment-top-level-reads.json',
      'config/parity-specs/shipping-fulfillments/delivery-settings-read.json',
      'config/parity-specs/shipping-fulfillments/carrier-service-lifecycle.json',
      'config/parity-specs/shipping-fulfillments/fulfillment-order-request-lifecycle.json',
      'config/parity-specs/shipping-fulfillments/fulfillment-service-lifecycle.json',
      'config/parity-specs/shipping-fulfillments/fulfillment-top-level-reads.json',
    ],
    cleanupBehavior:
      'Delegates to the parity cassette recorder for existing captured scenarios; mutation side effects remain local to the proxy and read cassettes are refreshed from Shopify.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    captureId: 'orphaned-store-property-fixtures',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-orphaned-store-property-fixtures-conformance.ts',
    purpose:
      'Re-records cassette-backed parity evidence for restored location validation fixture files that are consumed by standard parity specs.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/store-properties/location-lifecycle-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/store-properties/location-mutation-validation.json',
      'config/parity-specs/store-properties/location-activate-missing-idempotency-validation.json',
      'config/parity-specs/store-properties/location-deactivate-missing-idempotency-validation.json',
      'config/parity-specs/store-properties/location-delete-active-location-validation.json',
      'config/parity-specs/store-properties/location-add-blank-name-validation.json',
      'config/parity-specs/store-properties/location-edit-unknown-id-validation.json',
    ],
    cleanupBehavior:
      'Delegates to the parity cassette recorder for existing captured validation scenarios; the replayed validation branches do not create merchant resources.',
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
      'config/parity-requests/shipping-fulfillments/fulfillment-order-move-validation-hydrate.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-move-validation-move.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-move-validation-report-progress.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-move-validation-submit-request.graphql',
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
    captureId: 'fulfillment-order-merge-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-order-merge-validation-conformance.ts',
    purpose:
      'fulfillmentOrderMerge missing fulfillment order, invalid quantity/line-item, non-open fulfillment order, success, and downstream read-after-merge behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-order-merge-validation.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-order-merge-validation.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-merge-validation-order-read.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-merge-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable orders, splits fulfillment orders to produce mergeable pairs, captures validation and success branches, then cancels the orders.',
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
    captureId: 'fulfillment-service-delete-inventory-action-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-service-delete-inventory-action-validation-conformance.ts',
    purpose: 'fulfillmentServiceDelete KEEP/DELETE destinationLocationId validation and valid KEEP behavior.',
    requiredAuthScopes: ['read_fulfillments', 'write_fulfillments', 'read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-service-delete-inventory-action-validation.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-service-delete-inventory-action-validation.json',
    ],
    cleanupBehavior:
      'Creates a disposable destination location and fulfillment service; attempts to deactivate/delete both created locations after capture.',
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
    captureId: 'carrier-service-callback-url-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-carrier-service-callback-url-validation-conformance.ts',
    purpose:
      'DeliveryCarrierService callbackUrl variable coercion, HTTPS-only resolver validation, banned-host resolver validation, and update-time typed userError codes.',
    requiredAuthScopes: ['read_shipping', 'write_shipping'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}carrier-service-callback-url-validation.json`,
      'config/parity-specs/shipping-fulfillments/carrier-service-callback-url-validation.json',
      'config/parity-requests/shipping-fulfillments/carrier-service-callback-url-validation-update-banned.graphql',
      'config/parity-requests/shipping-fulfillments/carrier-service-callback-url-validation-update-http.graphql',
      'config/parity-requests/shipping-fulfillments/carrier-service-callback-url-validation.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable carrier service with an allowed callback URL, records invalid update attempts against it, then deletes the carrier service in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'delivery-profiles',
    scriptPath: 'scripts/capture-delivery-profile-conformance.ts',
    purpose: 'Delivery profile read/write lifecycle behavior.',
    requiredAuthScopes: ['read_shipping', 'write_shipping', 'delivery profile management access'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/delivery-profile-create-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/delivery-profile-writes.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/delivery-profiles-read.json',
      'config/parity-specs/shipping-fulfillments/delivery-profile-create-validation.json',
      'config/parity-specs/shipping-fulfillments/delivery-profile-lifecycle.json',
      'config/parity-specs/shipping-fulfillments/delivery-profile-read.json',
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
    captureId: 'bulk-operations-read-arg-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operations-read-arg-validation-conformance.ts',
    purpose: 'bulkOperations connection/search argument and bulkOperation id validation errors.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operations-read-arg-validation.json`,
      'config/parity-specs/bulk-operations/bulk-operations-read-arg-validation.json',
    ],
    cleanupBehavior: 'Validation-only capture; no Shopify data is created or mutated.',
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
      'config/parity-requests/bulk-operations/bulk-operation-run-query-group-objects-default.graphql',
      'config/parity-requests/bulk-operations/bulk-operation-run-query-group-objects-true.graphql',
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
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-cloud-uri-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-conformance.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-topic-enum-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-topic-format-name-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-uri-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-uri-whitespace.json',
    ],
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
    captureId: 'webhook-subscription-topic-enum-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-subscription-topic-enum-validation.ts',
    purpose: 'WebhookSubscriptionTopic enum coercion for unknown, hidden, variable, and accepted topic values.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-topic-enum-validation.json`,
      'config/parity-specs/webhooks/webhook-subscription-topic-enum-validation.json',
      'config/parity-requests/webhooks/webhook-subscription-bogus-topic.graphql',
      'config/parity-requests/webhooks/webhook-subscription-hidden-topic.graphql',
    ],
    cleanupBehavior:
      'Invalid enum branches fail before resolver side effects; accepted SHOP_UPDATE control creates one temporary HTTP webhook subscription and deletes it during cleanup.',
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
    captureId: 'gift-card-update-deactivated-multi-field',
    scriptPath: 'scripts/capture-gift-card-update-deactivated-multi-field-conformance.ts',
    purpose:
      'Gift-card update validation for deactivated cards when multiple blocked public fields are supplied in the same input.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-update-deactivated-multi-field.json`,
      'config/parity-specs/gift-cards/gift-card-update-deactivated-multi-field.json',
      'config/parity-requests/gift-cards/gift-card-update-deactivated-multi-field.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable customer plus one gift card, deactivates the gift card, records multi-field validation branches, and deletes the setup customer.',
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
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-account-page-data-erasure.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-add-tax-exemptions-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-address-country-province-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-address-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-by-identifier.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-create-input-id-rejected.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-create-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-create-rejects-nested-ids.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-delete-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-detail.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-input-addresses-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-input-inline-consent-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-input-validation-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-merge-attached-resources-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-merge-blockers.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-merge-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-nested-subresources.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-outbound-side-effect-validation-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-remove-tax-exemptions-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-replace-tax-exemptions-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-update-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-update-requires-identity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/customer-segment-members-query-create-validation-and-shape.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/segments/customer-segment-members-query-lifecycle.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/customers/customer-email-marketing-consent-update-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/customers/customer-invite-email-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/customers/customer-order-summary-read-effects.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/customers/customer-set-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/customers/customer-sms-marketing-consent-update-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customer-by-identifier.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customer-create-inline-missing-input.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customer-create-inline-null-input.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customer-create-missing-input.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customer-create-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customer-delete-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customer-detail.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customer-merge-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customer-nested-subresources.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customer-update-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/customers/customer-email-marketing-consent-update-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/customers/customer-sms-marketing-consent-update-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customers-advanced-search.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customers-catalog.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customers-count.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customers-relevance-search.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customers-search.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customers-sort-keys.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customers-advanced-search.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customers-catalog.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customers-count.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customers-relevance-search.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customers-search.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/customers/customers-sort-keys.json',
    ],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-mutations',
    scriptPath: 'scripts/capture-customer-mutation-conformance.mts',
    purpose: 'customerCreate/customerUpdate/customerDelete mutation family.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-create-parity.json`,
      `${CAPTURE_ROOT}customer-update-parity.json`,
      `${CAPTURE_ROOT}customer-delete-parity.json`,
    ],
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
      'config/parity-requests/customers/customerInputValidation-create.graphql',
      'config/parity-requests/customers/customerInputValidation-delete.graphql',
      'config/parity-requests/customers/customerInputValidation-downstream-read.graphql',
      'config/parity-requests/customers/customerInputValidation-merge.graphql',
      'config/parity-requests/customers/customerInputValidation-update.graphql',
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
      'config/parity-requests/customers/customerInputInlineConsent-create.graphql',
      'config/parity-requests/customers/customerInputInlineConsent-read.graphql',
      'config/parity-requests/customers/customerInputInlineConsent-update.graphql',
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
      'config/parity-requests/customers/customer_update_requires_identity_create.graphql',
      'config/parity-requests/customers/customer_update_requires_identity_read.graphql',
      'config/parity-requests/customers/customer_update_requires_identity_update.graphql',
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
      'config/parity-requests/customers/customer-input-addresses-create.graphql',
      'config/parity-requests/customers/customer-input-addresses-downstream-read.graphql',
      'config/parity-requests/customers/customer-input-addresses-update.graphql',
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
      'config/parity-requests/customers/customer-address-country-province-address-create.graphql',
      'config/parity-requests/customers/customer-address-country-province-address-update.graphql',
      'config/parity-requests/customers/customer-address-country-province-create.graphql',
      'config/parity-requests/customers/customer-address-country-province-set.graphql',
      'config/parity-requests/customers/customer-address-country-province-update.graphql',
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
    fixtureOutputs: ['fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/customers/customer-set-parity.json'],
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
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-address-country-province-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-address-lifecycle.json',
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
    captureId: 'customer-merge-blockers',
    scriptPath: 'scripts/capture-customer-merge-blockers-conformance.mts',
    purpose: 'Synchronous customerMerge blockers for combined tags, combined notes, and gift-card assignments.',
    requiredAuthScopes: [
      'read_customers',
      'write_customers',
      'read_customer_merge',
      'write_customer_merge',
      'read_gift_cards',
      'write_gift_cards',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-merge-blockers.json`,
      'config/parity-specs/customers/customerMerge-blockers.json',
    ],
    cleanupBehavior:
      'Creates disposable customers and one assigned gift card; deactivates the gift card and deletes customers after validation.',
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
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-add-tax-exemptions-parity.json`,
      `${CAPTURE_ROOT}customer-remove-tax-exemptions-parity.json`,
      `${CAPTURE_ROOT}customer-replace-tax-exemptions-parity.json`,
    ],
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
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/customers/customer-order-summary-read-effects.json',
    ],
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
      'config/parity-requests/customers/customer-invite-email-validation-create.graphql',
      'config/parity-requests/customers/customer-invite-email-validation-invite.graphql',
      'config/parity-requests/customers/customer-invite-email-validation-read.graphql',
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

export type ConformanceFixtureProvenanceProfile = {
  fixtureCount: number;
  liveShopifyFixtureCount: number;
  localRuntimeFixtureCount: number;
  indexedFixtureOutputPatterns: string[];
  orphanedFixturePaths: string[];
};

export function listConformanceFixturePaths(repoRoot = process.cwd()): string[] {
  const fixtureRoot = path.join(repoRoot, 'fixtures', 'conformance');

  return walkFiles(fixtureRoot)
    .filter((filePath) => filePath.endsWith('.json'))
    .map((filePath) => path.relative(repoRoot, filePath).split(path.sep).join('/'))
    .sort();
}

export function listIndexedConformanceFixtureOutputPatterns(
  entries: ConformanceCaptureIndexEntry[] = conformanceCaptureIndex,
): string[] {
  return [
    ...new Set(
      entries.flatMap((entry) => entry.fixtureOutputs).filter((output) => output.startsWith('fixtures/conformance/')),
    ),
  ].sort();
}

export function profileConformanceFixtureProvenance(
  repoRoot = process.cwd(),
  entries: ConformanceCaptureIndexEntry[] = conformanceCaptureIndex,
): ConformanceFixtureProvenanceProfile {
  const fixturePaths = listConformanceFixturePaths(repoRoot);
  const liveShopifyFixturePaths = fixturePaths.filter((fixturePath) => !isLocalRuntimeFixturePath(fixturePath));
  const indexedFixtureOutputPatterns = listIndexedConformanceFixtureOutputPatterns(entries);
  const indexedFixtureOutputMatchers = indexedFixtureOutputPatterns.map(fixtureOutputPatternToRegExp);

  return {
    fixtureCount: fixturePaths.length,
    liveShopifyFixtureCount: liveShopifyFixturePaths.length,
    localRuntimeFixtureCount: fixturePaths.length - liveShopifyFixturePaths.length,
    indexedFixtureOutputPatterns,
    orphanedFixturePaths: liveShopifyFixturePaths.filter(
      (fixturePath) => !indexedFixtureOutputMatchers.some((matcher) => matcher.test(fixturePath)),
    ),
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

function walkFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    return entry.isDirectory() ? walkFiles(entryPath) : [entryPath];
  });
}

function isLocalRuntimeFixturePath(fixturePath: string): boolean {
  return fixturePath.startsWith('fixtures/conformance/local-runtime/');
}

function fixtureOutputPatternToRegExp(fixtureOutputPattern: string): RegExp {
  const normalizedPattern = fixtureOutputPattern
    .replaceAll('<store-domain>', '<store>')
    .replaceAll('<storeDomain>', '<store>');
  let source = '';

  for (let index = 0; index < normalizedPattern.length; ) {
    const placeholder = ['<store>', '<api-version>', '<domain-folder>'].find((candidate) =>
      normalizedPattern.startsWith(candidate, index),
    );

    if (placeholder) {
      source += '[^/]+';
      index += placeholder.length;
    } else {
      source += escapeRegExpLiteral(normalizedPattern[index] ?? '');
      index += 1;
    }
  }

  return new RegExp(`^${source}$`, 'u');
}

function escapeRegExpLiteral(value: string): string {
  return value.replace(/[\\^$.*+?()[\]{}|]/gu, '\\$&');
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
