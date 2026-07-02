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
  'events',
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
  'selling-plans',
  'store-properties',
  'webhooks',
]);

const statusCheckSchema = z.enum([
  'conformance:status',
  'conformance:check',
  'conformance:parity',
  'rust:test',
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

const DEFAULT_STATUS_CHECKS: StatusCheck[] = ['conformance:status', 'conformance:check', 'rust:test'];
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
    captureId: 'assign-customer-as-contact-error-branches',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-assign-customer-as-contact-error-branches-conformance.mts',
    purpose:
      'B2B companyAssignCustomerAsContact userError branches for unknown customer, duplicate customer contact, and existing customer without email.',
    requiredAuthScopes: ['read_companies', 'write_companies', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}assign-customer-as-contact-no-email-invalid-input.json`,
      'config/parity-specs/b2b/assign_customer_as_contact_no_email_invalid_input.json',
      'config/parity-requests/b2b/assign-customer-as-contact-error-branch-company-create.graphql',
      'config/parity-requests/b2b/assign-customer-as-contact-error-branch-customer-create.graphql',
      'config/parity-requests/b2b/assign-customer-as-contact-error-branch-assign.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable B2B company, one normal disposable customer, and one disposable phone-only customer; records validation branches, then deletes all created records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'orphaned-b2b-custom-data-fixtures',
    scriptPath: 'scripts/capture-orphaned-b2b-custom-data-fixtures-conformance.ts',
    purpose:
      'Recorder provenance replacement for legacy B2B, metafield definition, saved-search, segment, localization, and metaobject fixture files that already have parity consumers or gain one here.',
    requiredAuthScopes: [
      'read_companies',
      'write_companies',
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
      'read_metaobjects',
      'write_metaobjects',
      'customer segment access',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-company-mutation-validation.json`,
      `${CAPTURE_ROOT}b2b-company-roots-read.json`,
      `${CAPTURE_ROOT}metafield-definitions-product-read.json`,
      `${CAPTURE_ROOT}saved-search-url-redirects.json`,
      `${CAPTURE_ROOT}segment-lifecycle-validation.json`,
      `${CAPTURE_ROOT}b2b-company-create-lifecycle.json`,
      `${CAPTURE_ROOT}localization-locale-translation-fixture.json`,
      `${CAPTURE_ROOT}metaobject-create-cold-hydration.json`,
      'config/parity-specs/b2b/b2b-company-mutation-validation.json',
      'config/parity-requests/b2b/b2b-company-mutation-validation.graphql',
      'config/parity-requests/b2b/b2b-company-roots-read.variables.json',
    ],
    cleanupBehavior:
      'Creates disposable B2B companies, saved searches, translations, metaobject definitions, and metaobjects as needed, then deletes or disables each created live object before exit; validation-only branches use unknown IDs and do not mutate Shopify.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'b2b-update-unknown-id-resource-not-found',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-update-unknown-id-resource-not-found-conformance.mts',
    purpose:
      'B2B companyUpdate, companyLocationUpdate, and companyLocationTaxSettingsUpdate RESOURCE_NOT_FOUND payloads for never-created IDs.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-update-unknown-id-resource-not-found.json`,
      'config/parity-specs/b2b/b2b-update-unknown-id-resource-not-found.json',
      'config/parity-requests/b2b/b2b-update-unknown-id-company-update.graphql',
      'config/parity-requests/b2b/b2b-update-unknown-id-location-update.graphql',
      'config/parity-requests/b2b/b2b-update-unknown-id-tax-settings.graphql',
    ],
    cleanupBehavior:
      'Uses fixed never-created Company and CompanyLocation GIDs only; Shopify rejects before mutation so no cleanup is required.',
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
    captureId: 'b2b-company-location-tax-settings-sequential',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-company-location-tax-settings-sequential-conformance.mts',
    purpose:
      'B2B companyLocationTaxSettingsUpdate registration-only updates, no-knob no-ops, exemption assign/remove set math, omitted taxExempt preservation, and taxSettings read-after-write.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-company-location-tax-settings-sequential.json`,
      'config/parity-specs/b2b/b2b-company-location-tax-settings-sequential.json',
    ],
    cleanupBehavior:
      'Creates one disposable B2B company/location, records registration-only, no-knob, and exemption tax settings updates and reads, then deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'b2b-buyer-experience-configuration',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-buyer-experience-configuration-conformance.mts',
    purpose:
      'B2B CompanyLocation buyerExperienceConfiguration create/update storage, downstream reads, empty BEC validation, and deposit/payment-terms preconditions.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-buyer-experience-configuration.json`,
      'config/parity-specs/b2b/b2b-buyer-experience-configuration.json',
      'config/parity-requests/b2b/b2b-buyer-experience-company-create.graphql',
      'config/parity-requests/b2b/b2b-buyer-experience-location-create.graphql',
      'config/parity-requests/b2b/b2b-buyer-experience-location-update.graphql',
      'config/parity-requests/b2b/b2b-buyer-experience-company-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable B2B company with two locations, records validation and successful BEC branches, reads back the locations, then deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The current conformance shop accepts deposit with paymentTermsTemplateId; the disabled-shop deposit_not_enabled guard remains runtime-test-backed.',
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
    captureId: 'b2b-bulk-role-assign-duplicates',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-bulk-role-assign-duplicates-conformance.mts',
    purpose:
      'B2B bulk companyContactAssignRoles and companyLocationAssignRoles duplicate contact/location role-assignment LIMIT_REACHED behavior with valid sibling entries.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-bulk-role-assign-duplicates.json`,
      'config/parity-specs/b2b/b2b-bulk-role-assign-duplicates.json',
      'config/parity-requests/b2b/b2b-bulk-role-assign-duplicate-contact-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable B2B company with two extra locations and one extra contact, records duplicate bulk role-assignment branches, then deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
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
      'config/parity-specs/b2b/b2b-revoke-role-scope-regression-branches.json',
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
    purpose: 'B2B company/contact/location free-text length, blank-name, and HTML validation branches.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-string-validation.json`,
      'config/parity-specs/b2b/b2b-string-validation.json',
      'config/parity-requests/b2b/b2b-string-validation-company-create.graphql',
      'config/parity-requests/b2b/b2b-string-validation-company-read.graphql',
      'config/parity-requests/b2b/b2b-string-validation-company-update.graphql',
      'config/parity-requests/b2b/b2b-string-validation-location-create.graphql',
      'config/parity-requests/b2b/b2b-string-validation-location-update.graphql',
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
    captureId: 'b2b-location-input-normalization',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-location-input-normalization-conformance.mts',
    purpose:
      'B2B company location phone normalization, create-time locale defaulting, update-time locale preservation, and malformed locale passthrough for nested companyCreate, companyLocationCreate, and companyLocationUpdate inputs.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-location-input-normalization.json`,
      'config/parity-specs/b2b/b2b-location-input-normalization.json',
      'config/parity-requests/b2b/b2b-location-input-normalization-company-create.graphql',
      'config/parity-requests/b2b/b2b-location-input-normalization-location-create.graphql',
      'config/parity-requests/b2b/b2b-location-input-normalization-location-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable companies with B2B locations, deletes the extra malformed-locale nested company immediately, then deletes the main company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'company-location-name-fallback',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-company-location-name-fallback-conformance.mts',
    purpose:
      'Divergent B2B location name fallbacks for nested companyCreate(input.companyLocation) versus standalone companyLocationCreate when no location name is supplied and shippingAddress.address1 is present.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}company_location_name_fallback.json`,
      'config/parity-specs/b2b/company_create_location_name_fallback.json',
      'config/parity-specs/b2b/company_location_create_name_fallback.json',
    ],
    cleanupBehavior:
      'Creates two disposable B2B companies, records the nested and standalone location-name fallback payloads, then deletes both companies during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'b2b-contact-update-customer-readback',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-contact-update-customer-readback-conformance.mts',
    purpose:
      'B2B companyContactUpdate read-after-write behavior for the linked CompanyContact.customer subobject after firstName, lastName, email, and phone changes.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-contact-update-customer-readback.json`,
      'config/parity-specs/b2b/b2b-contact-update-customer-readback.json',
      'config/parity-requests/b2b/b2b-contact-update-customer-readback-company-create.graphql',
      'config/parity-requests/b2b/b2b-contact-update-customer-readback-contact-create.graphql',
      'config/parity-requests/b2b/b2b-contact-update-customer-readback-contact-update.graphql',
      'config/parity-requests/b2b/b2b-contact-update-customer-readback-contact-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable company with a B2B contact, updates the contact, records downstream readback, then deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'b2b-contact-email-name-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-contact-email-name-validation-conformance.mts',
    purpose:
      'B2B contact email format validation plus public Admin contact-name validation payloads for create, update, and nested companyCreate inputs.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-contact-email-name-validation.json`,
      'config/parity-specs/b2b/b2b-contact-email-name-validation.json',
      'config/parity-requests/b2b/b2b-contact-email-name-validation-company-create.graphql',
      'config/parity-requests/b2b/b2b-contact-email-name-validation-contact-create.graphql',
      'config/parity-requests/b2b/b2b-contact-email-name-validation-contact-update.graphql',
    ],
    cleanupBehavior: 'Creates one disposable company with a B2B contact, then deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Public Admin 2026-04 exposes BusinessCustomerUserError field/message/code but not detail; emoji and URL name inputs have live behavior that differs from older internal expectations, so they remain runtime-test-backed local guardrails.',
  },
  {
    domain: 'b2b',
    captureId: 'b2b-billing-same-as-shipping-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-billing-same-as-shipping-conformance.mts',
    purpose:
      'B2B billingSameAsShipping billingAddress/shippingAddress mutual-exclusion and taxExempt null validation for company location create/update inputs.',
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
    captureId: 'location-assign-address-preserves-id',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-location-assign-address-preserves-id-conformance.mts',
    purpose: 'B2B companyLocationAssignAddress first-assign creation and update-branch CompanyAddress ID preservation.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location_assign_address_preserves_id.json`,
      'config/parity-specs/b2b/location_assign_address_preserves_id.json',
      'config/parity-requests/b2b/location-assign-address-preserves-id-assign.graphql',
      'config/parity-requests/b2b/location-assign-address-preserves-id-company-create.graphql',
      'config/parity-requests/b2b/location-assign-address-preserves-id-location-create.graphql',
      'config/parity-requests/b2b/location-assign-address-preserves-id-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable company with one empty-address location and one dual-address location; deletes the company during scenario cleanup.',
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
      'config/parity-specs/b2b/contact_assign_role_both_invalid.json',
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
    captureId: 'b2b-location-delete-deletable-check',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-location-delete-deletable-check-conformance.mts',
    purpose:
      'B2B companyLocationDelete and companyLocationsDelete failed-deletable checks for only-location, draft-order, store-credit, and bulk partial-success branches.',
    requiredAuthScopes: [
      'read_companies',
      'write_companies',
      'write_draft_orders',
      'read_store_credit_accounts',
      'write_store_credit_account_transactions',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-delete-failed-deletable-check.json`,
      'config/parity-specs/b2b/location_delete_failed_deletable_check.json',
      'config/parity-specs/b2b/locations_delete_failed_deletable_check.json',
      'config/parity-requests/b2b/location-delete-check-bulk-read.graphql',
      'config/parity-requests/b2b/location-delete-check-company-create.graphql',
      'config/parity-requests/b2b/location-delete-check-draft-order-create.graphql',
      'config/parity-requests/b2b/location-delete-check-location-create.graphql',
      'config/parity-requests/b2b/location-delete-check-location-delete.graphql',
      'config/parity-requests/b2b/location-delete-check-locations-delete.graphql',
      'config/parity-requests/b2b/location-delete-check-read.graphql',
      'config/parity-requests/b2b/location-delete-check-store-credit-credit.graphql',
    ],
    cleanupBehavior:
      'Creates disposable B2B companies and locations, creates an open B2B draft order, creates and debits a company-location store credit account, records rejected delete branches and a bulk partial-success branch, then deletes the draft order and companies during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'b2b',
    captureId: 'b2b-company-delete-deletable-check',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-company-delete-deletable-check-conformance.mts',
    purpose:
      'B2B companyDelete and companiesDelete failed-deletable checks for order-history, draft-order, store-credit, and bulk partial-success branches.',
    requiredAuthScopes: [
      'read_companies',
      'write_companies',
      'write_draft_orders',
      'write_orders',
      'read_store_credit_accounts',
      'write_store_credit_account_transactions',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}company-delete-failed-deletable-check.json`,
      'config/parity-specs/b2b/company_delete_failed_deletable_check.json',
      'config/parity-specs/b2b/companies_delete_failed_deletable_check.json',
      'config/parity-requests/b2b/company-delete-check-bulk-read.graphql',
      'config/parity-requests/b2b/company-delete-check-companies-delete.graphql',
      'config/parity-requests/b2b/company-delete-check-company-create.graphql',
      'config/parity-requests/b2b/company-delete-check-company-delete.graphql',
      'config/parity-requests/b2b/company-delete-check-draft-order-complete.graphql',
      'config/parity-requests/b2b/company-delete-check-draft-order-create.graphql',
      'config/parity-requests/b2b/company-delete-check-single-read.graphql',
      'config/parity-requests/b2b/company-delete-check-store-credit-credit.graphql',
    ],
    cleanupBehavior:
      'Creates disposable B2B companies, creates open and completed B2B draft orders, creates and debits company-location store credit accounts, records rejected company delete branches and a bulk partial-success branch, cancels/deletes completed orders when Shopify accepts it, then attempts company cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Shopify can retain completed-order company history even after order deletion; order-history cleanup companyDelete responses are recorded and may remain blocked.',
  },
  {
    domain: 'b2b',
    captureId: 'b2b-no-input-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-no-input-validation-conformance.mts',
    purpose:
      'B2B empty-object input validation for company/contact/location update roots, company contact create, and address-only company location create, plus readback proving the validation branches are no-ops.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-no-input-validation.json`,
      'config/parity-specs/b2b/b2b-no-input-validation.json',
      'config/parity-requests/b2b/b2b-no-input-validation-company-create.graphql',
      'config/parity-requests/b2b/b2b-no-input-validation-company-read.graphql',
      'config/parity-requests/b2b/b2b-no-input-validation-company-update.graphql',
      'config/parity-requests/b2b/b2b-no-input-validation-contact-create.graphql',
      'config/parity-requests/b2b/b2b-no-input-validation-contact-update.graphql',
      'config/parity-requests/b2b/b2b-no-input-validation-location-create.graphql',
      'config/parity-requests/b2b/b2b-no-input-validation-location-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable company with a contact and location, records validation failures and unchanged readback, then deletes the company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The capture also records null-only probes showing the public schema does not treat null-only keys as a uniform NO_INPUT branch, plus address-only companyLocationCreate evidence for NO_INPUT.',
  },
  {
    domain: 'b2b',
    captureId: 'b2b-contact-missing-email-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-b2b-contact-missing-email-validation-conformance.mts',
    purpose:
      'B2B companyContactCreate and nested companyCreate companyContact missing-email validation, including unchanged standalone company readback and empty nested company search evidence.',
    requiredAuthScopes: ['read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}b2b-contact-missing-email-validation.json`,
      'config/parity-specs/b2b/b2b-contact-missing-email-validation.json',
      'config/parity-requests/b2b/b2b-contact-missing-email-validation-company-create.graphql',
      'config/parity-requests/b2b/b2b-contact-missing-email-validation-contact-create.graphql',
      'config/parity-requests/b2b/b2b-contact-missing-email-validation-company-read.graphql',
      'config/parity-requests/b2b/b2b-contact-missing-email-validation-companies-search.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable setup company, records missing-email validation failures and readback/search no-op evidence, then deletes the setup company during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The nested rejection has no returned company ID, so empty read-after-write evidence uses a companies query by the rejected company name.',
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
    captureId: 'products-common-search-filters',
    scriptPath: 'scripts/capture-products-common-search-filters-conformance.ts',
    purpose:
      'Products/productsCount common search filters for status, vendor, and product_type against live Shopify catalog data.',
    requiredAuthScopes: ['read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}products-common-search-filters.json`,
      'config/parity-specs/products/products-common-search-filters-read.json',
      'config/parity-requests/products/products-common-search-filters-read.graphql',
    ],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Records the exact products/productsCount read as an upstream cassette for cold live-hybrid replay; local store-state filtering is covered by focused Rust tests.',
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
      'config/parity-specs/products/productCreate-blank-title-parity.json',
      'config/parity-specs/products/productCreate-parity-plan.json',
      'config/parity-specs/products/productDelete-parity-plan.json',
      'config/parity-specs/products/productDelete-unknown-id-parity.json',
      'config/parity-requests/products/productCreate-parity-plan.graphql',
      'config/parity-requests/products/productDelete-parity-plan.graphql',
    ],
    cleanupBehavior: 'Creates disposable products and deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-create-input-fields',
    scriptPath: 'scripts/capture-product-create-input-fields-conformance.ts',
    purpose:
      'productCreate category, requiresSellingPlan, and collectionsToJoin staging with immediate downstream readback.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}productCreate-category-parity.json`,
      `${CAPTURE_ROOT}productCreate-requires-selling-plan-parity.json`,
      `${CAPTURE_ROOT}productCreate-collections-to-join-parity.json`,
      'config/parity-specs/products/productCreate-category-parity.json',
      'config/parity-specs/products/productCreate-requires-selling-plan-parity.json',
      'config/parity-specs/products/productCreate-collections-to-join-parity.json',
      'config/parity-requests/products/productCreate-category-parity.graphql',
      'config/parity-requests/products/productCreate-category-downstream-read.graphql',
      'config/parity-requests/products/productCreate-requires-selling-plan-parity.graphql',
      'config/parity-requests/products/productCreate-requires-selling-plan-downstream-read.graphql',
      'config/parity-requests/products/productCreate-collections-to-join-collection-create.graphql',
      'config/parity-requests/products/productCreate-collections-to-join-parity.graphql',
      'config/parity-requests/products/productCreate-collections-to-join-downstream-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable products and two disposable custom collections, captures validation probes, then deletes created products and collections in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Current public 2025-01 accepts productType alongside category and ignores unknown collectionsToJoin IDs; the recorder preserves that observed behavior in validation captures.',
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
    captureId: 'product-create-dropped-inputs',
    scriptPath: 'scripts/capture-product-create-dropped-inputs-conformance.ts',
    purpose:
      'productCreate giftCard, giftCardTemplateSuffix, claimOwnership.bundles, metafields, and productPublications staging with immediate downstream readback.',
    requiredAuthScopes: ['read_products', 'write_products', 'publication/channel access for the app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}productCreate-dropped-inputs-parity.json`,
      'config/parity-specs/products/productCreate-dropped-inputs-parity.json',
      'config/parity-requests/products/productCreate-dropped-inputs-parity.graphql',
      'config/parity-requests/products/productCreate-dropped-inputs-downstream-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable gift-card/metafield and publication-staged products, then deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-create-no-key-on-create',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-product-create-no-key-on-create-conformance.ts',
    purpose:
      'productCreate legacy ProductInput key-on-create guardrails for input.id precedence and ProductInput variants rejection.',
    requiredAuthScopes: ['write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-create-no-key-on-create.json`,
      'config/parity-specs/products/product-create-no-key-on-create.json',
      'config/parity-requests/products/product-create-no-key-on-create.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; input.id and input.variants branches must not create Shopify products.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Public Admin 2026-04 returns a productCreate.userErrors payload for legacy input.id and rejects input.variants during input coercion because ProductInput does not define variants.',
  },
  {
    domain: 'products',
    captureId: 'collection-create-rejects-id',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-collection-create-rejects-id-conformance.ts',
    purpose:
      'collectionCreate validation for caller-supplied CollectionInput.id returning a payload userError without creating a collection.',
    requiredAuthScopes: ['write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}collection-create-rejects-id.json`,
      'config/parity-specs/products/collectionCreate-rejects-id.json',
      'config/parity-requests/products/collectionCreate-rejects-id.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; rejected input.id must not create a Shopify collection.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'collection-create-products-ruleset-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-collection-create-products-ruleset-validation-conformance.ts',
    purpose:
      'collectionCreate behavior for unknown input.products, accepted empty ruleSet.rules, and omitted ruleSet.rules validation.',
    requiredAuthScopes: ['write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}collection-create-products-ruleset-validation.json`,
      'config/parity-specs/products/collectionCreate-unknown-products.json',
      'config/parity-specs/products/collectionCreate-empty-ruleset-rules.json',
      'config/parity-requests/products/collectionCreate-products-ruleset-validation.graphql',
    ],
    cleanupBehavior:
      'Validation-first capture; unknown products and omitted ruleSet.rules must not create Shopify collections, while the accepted empty-rules custom collection is deleted during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'collection-delete-shop-payload',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-collection-delete-shop-payload-conformance.ts',
    purpose:
      'collectionDelete payload shape for the required non-null shop field on success and not-found userError branches.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}collection-delete-shop-payload.json`,
      'config/parity-specs/products/collectionDelete-parity-plan.json',
      'config/parity-requests/products/collectionDelete-parity-plan.graphql',
    ],
    cleanupBehavior: 'Creates one disposable collection and deletes it; not-found branch is validation-only.',
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
      'config/parity-specs/products/productSet-parity-plan.json',
      `${CAPTURE_ROOT}product-set-shape-validator-parity.json`,
      `${CAPTURE_ROOT}product-set-async-operation-parity.json`,
      `${CAPTURE_ROOT}product-set-id-not-allowed.json`,
      'config/parity-specs/products/productSet-shape-validator-parity.json',
      'config/parity-specs/products/productSet-async-operation-parity.json',
      'config/parity-specs/products/product-set-id-not-allowed.json',
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
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/tags-normalization-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/tags-add-parity.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/products/tags-remove-parity.json',
      'config/parity-specs/products/tagsAdd-parity-plan.json',
      'config/parity-specs/products/tags-normalization-parity.json',
      'config/parity-requests/products/tags-normalization-setup.graphql',
      'config/parity-requests/products/tagsAdd-case-variant.graphql',
      'config/parity-requests/products/tagsAdd-comma-list-element.graphql',
      'config/parity-requests/products/tagsAdd-comma-string.graphql',
      'config/parity-requests/products/tagsRemove-case-variant.graphql',
      'config/parity-requests/products/tagsRemove-string.graphql',
    ],
    cleanupBehavior: 'Creates temporary products and resets/deletes them in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-change-status-unknown-product',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-product-change-status-unknown-product-conformance.mts',
    purpose: 'productChangeStatus unknown-product resolver userError shape, including PRODUCT_NOT_FOUND code.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-change-status-unknown-product-parity.json`,
      'config/parity-specs/products/productChangeStatus-unknown-product-parity.json',
      'config/parity-requests/products/productChangeStatus-parity-plan.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture against a non-existent product id; Shopify returns a userError and creates no product state.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-status-enum-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-product-status-enum-validation-conformance.ts',
    purpose:
      'ProductStatus enum schema-validation errors for invalid productChangeStatus status arguments and productCreate input.status values.',
    requiredAuthScopes: ['read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-status-enum-validation.json`,
      'config/parity-specs/products/product-status-enum-validation.json',
      'config/parity-requests/products/productChangeStatus-invalid-status-literal.graphql',
      'config/parity-requests/products/productChangeStatus-invalid-status-variable.graphql',
      'config/parity-requests/products/productCreate-invalid-status-literal.graphql',
      'config/parity-requests/products/productCreate-invalid-status-variable.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture; invalid enum inputs are rejected by Shopify schema validation before resolver execution and do not create or mutate products.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-feedback-validation-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-product-feedback-validation-local-runtime.ts',
    purpose:
      'Local-runtime resource-feedback validation parity for invalid enum literals, message validation, future timestamps, mixed product batches, batch caps, and shop feedback guardrails.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}product-feedback-lifecycle-local-runtime.json`,
      `${LOCAL_RUNTIME_ROOT}product-feedback-validation-local-runtime.json`,
      'config/parity-specs/products/bulk_product_resource_feedback_create_validation.json',
      'config/parity-requests/products/product-feedback-create-local-runtime.graphql',
      'config/parity-requests/products/product-feedback-invalid-state.graphql',
      'config/parity-requests/products/shop-feedback-create-local-runtime.graphql',
      'config/parity-requests/products/shop-feedback-invalid-state.graphql',
    ],
    cleanupBehavior: 'Local-runtime validation-only capture; no Shopify or local cleanup is required.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Live resolver capture is blocked by the conformance app resource-feedback access and sales-channel configuration, so this fixture records executable local-runtime evidence for the supported no-upstream mutation contract.',
  },
  {
    domain: 'products',
    captureId: 'product-metafield-delete-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-product-metafield-delete-local-runtime.ts',
    purpose:
      'Executable product-owner local-runtime parity for singular metafieldDelete success, downstream read-after-delete, and not-found payloads.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}metafield-delete-product-owner-local-runtime.json`,
      'config/parity-specs/products/metafieldDelete-product-owner-local-runtime.json',
      'config/parity-requests/products/metafieldDelete-product-owner-setup.graphql',
      'config/parity-requests/products/metafieldDelete-product-owner.graphql',
      'config/parity-requests/products/metafieldDelete-product-owner-read.graphql',
    ],
    cleanupBehavior: 'Local-runtime only; supported mutations stage locally and no Shopify cleanup is required.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Live public Admin GraphQL 2026-04 and unstable expose plural metafieldsDelete but not singular metafieldDelete, so the singular compatibility alias is covered by local-runtime parity plus live plural-root evidence.',
  },
  {
    domain: 'metafields',
    captureId: 'metafield-delete-not-found-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-delete-not-found-local-runtime.ts',
    purpose:
      'Executable local-runtime parity for singular metafieldDelete happy, repeat-delete, and never-created not-found payloads.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      'fixtures/conformance/local-runtime/2026-04/metafield-definitions/metafield-delete-not-found.json',
      'config/parity-specs/metafield-definitions/metafield-delete-not-found.json',
      'config/parity-requests/metafield-definitions/metafield-delete-not-found-setup.graphql',
      'config/parity-requests/metafield-definitions/metafield-delete-by-id.graphql',
    ],
    cleanupBehavior: 'Local-runtime only; supported mutations stage locally and no Shopify cleanup is required.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Live public Admin GraphQL exposes plural metafieldsDelete but not singular metafieldDelete, so this compatibility-root not-found branch is local-runtime-backed.',
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
    captureId: 'publication-mutation-contract',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-publication-mutation-contract-conformance.mts',
    purpose:
      'publicationCreate/publicationUpdate/publicationDelete 2026-04 input shape, userErrors, and delete payload contract.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_publications', 'write_publications'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}publication-mutation-contract.json`,
      'config/parity-specs/products/publication_create_validation.json',
      'config/parity-specs/products/publication-update-delete-contract.json',
      'config/parity-requests/products/publicationCreate-validation.graphql',
      'config/parity-requests/products/publicationUpdate-contract.graphql',
      'config/parity-requests/products/publicationDelete-contract.graphql',
      'config/parity-requests/products/products-hydrate-nodes-observation.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable draft product and one disposable publication, deletes the publication as the asserted delete case, then deletes the product in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-publications',
    scriptPath: 'scripts/capture-product-publication-conformance.mts',
    purpose: 'Publication aggregate reads plus productPublish/productUnpublish probes.',
    requiredAuthScopes: ['read_products', 'write_products', 'publication/channel access for the app'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}publication-roots-local-runtime.json`,
      'config/parity-specs/products/publication-roots-local-runtime.json',
      'config/parity-requests/products/publicationCreate-local-runtime.graphql',
      'config/parity-requests/products/publicationUpdate-local-runtime.graphql',
      'config/parity-requests/products/publicationDelete-local-runtime.graphql',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/publications-catalog.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-publish-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-unpublish-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-publish-unpublish.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/publishable-publish-current-shop-count-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/publishable-publish-shop-count-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/publishable-unpublish-current-shop-count-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/publishable-unpublish-shop-count-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/store-properties/publishable-publish-current-shop-count-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/store-properties/publishable-publish-shop-count-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/store-properties/publishable-unpublish-current-shop-count-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/store-properties/publishable-unpublish-shop-count-parity.json',
      'config/parity-specs/products/product-publish-unpublish.json',
      'config/parity-specs/store-properties/publishablePublish-shop-count-parity.json',
      'config/parity-specs/store-properties/publishablePublishToCurrentChannel-shop-count-parity.json',
      'config/parity-specs/store-properties/publishableUnpublish-shop-count-parity.json',
      'config/parity-specs/store-properties/publishableUnpublishToCurrentChannel-shop-count-parity.json',
      'config/parity-requests/products/product-publish-unpublish-publish.graphql',
      'config/parity-requests/products/product-publish-unpublish-unpublish.graphql',
      'config/parity-requests/products/product-publish-unpublish-downstream-read.graphql',
      'config/parity-requests/products/publication-resource-hydrate-nodes.graphql',
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
    domain: 'store-properties',
    captureId: 'publishable-input-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-publishable-input-validation-conformance.ts',
    purpose:
      'Generic publishable PublicationInput validation for duplicate publicationId, blank publicationId, pre-1970 publishDate, unknown publicationId, and current-channel id-only sibling behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/store-properties/publishable-input-validation.json',
      'config/parity-specs/store-properties/publishable-input-validation.json',
      'config/parity-requests/store-properties/publishable-input-validation.graphql',
      'config/parity-requests/store-properties/publishable-input-validation-unpublish.graphql',
      'config/parity-requests/store-properties/publishable-input-validation-publish-current.graphql',
      'config/parity-requests/store-properties/publishable-input-validation-unpublish-current.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable draft product, records generic publishable validation branches and current-channel sibling payloads, captures a hydration cassette while the product exists, then deletes the product.',
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
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-create-media-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-update-media-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-delete-media-parity.json',
      'config/parity-requests/products/productCreateMedia-validation-branches.graphql',
      'config/parity-requests/products/productCreateMedia-dual-userErrors.graphql',
      'config/parity-requests/products/productUpdateMedia-validation-branches.graphql',
      'config/parity-requests/products/productDeleteMedia-validation-branches.graphql',
      'config/parity-requests/products/productReorderMedia-validation-branches.graphql',
      'config/parity-specs/products/product-media-validation-branches.json',
      'config/parity-specs/products/productCreateMedia-dual-userErrors.json',
    ],
    cleanupBehavior: 'Creates disposable product/media records and deletes the product during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-media-missing-media-aggregation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-product-media-missing-media-aggregation-conformance.mts',
    purpose:
      'Product media update/delete validation for aggregating multiple missing media IDs into one MEDIA_DOES_NOT_EXIST error.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-media-missing-media-aggregation.json',
      'config/parity-requests/products/productUpdateMedia-missing-media-aggregation.graphql',
      'config/parity-requests/products/productDeleteMedia-missing-media-aggregation.graphql',
      'config/parity-specs/products/product-media-missing-media-aggregation.json',
    ],
    cleanupBehavior:
      'Creates one disposable draft product with one product media node, records update/delete requests for two nonexistent media IDs, then deletes the product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'file-mutations',
    scriptPath: 'scripts/capture-file-mutation-conformance.mts',
    purpose: 'fileCreate/fileUpdate/fileDelete and staged upload interactions.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}file-update-product-reference-local-runtime.json`,
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
    captureId: 'media-file-interface-fields',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-interface-fields-conformance.mts',
    purpose:
      'Files API File interface non-null fields and type-specific MediaImage/GenericFile field projection for fileCreate plus files read-after-write.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-interface-fields.json`,
      'config/parity-specs/media/media-file-interface-fields.json',
      'config/parity-requests/media/media-file-interface-fields-create.graphql',
      'config/parity-requests/media/media-file-interface-fields-read.graphql',
    ],
    cleanupBehavior: 'Creates one disposable image file and one disposable generic file, then deletes both.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'media-file-create-content-type-inference',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-create-content-type-inference-conformance.ts',
    purpose:
      'fileCreate omitted-contentType inference for image, video, document, 3D model, and extensionless source URLs plus downstream files/node read-after-write.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}file-create-content-type-inference.json`,
      'config/parity-specs/media/file_create_content_type_inference.json',
      'config/parity-requests/media/file-create-content-type-inference-create.graphql',
      'config/parity-requests/media/file-create-content-type-inference-files-read.graphql',
      'config/parity-requests/media/file-create-content-type-inference-video-node.graphql',
      'config/parity-requests/media/file-create-content-type-inference-generic-node.graphql',
    ],
    cleanupBehavior: 'Creates disposable image, video, document, 3D model, and extensionless files, then deletes them.',
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
    captureId: 'media-file-update-validation-ordering',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-update-validation-ordering.ts',
    purpose:
      'Files API fileUpdate validation bucket ordering for missing ids, non-ready files, long alt, and simultaneous source fields.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-update-validation-ordering.json`,
      'config/parity-specs/media/file_update_validation_ordering.json',
    ],
    cleanupBehavior: 'Creates one disposable image file and deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'media-file-update-filename-extension-aggregation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-update-filename-extension-aggregation.ts',
    purpose:
      'Files API fileUpdate filename validation aggregation for single and multi-input image extension mismatches plus multi-input unsupported external-video filename updates.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-update-filename-extension-aggregation.json`,
      'config/parity-specs/media/file_update_filename_extension/filename-extension-aggregation.json',
      'config/parity-requests/media/file_update_filename_extension/file-update.graphql',
    ],
    cleanupBehavior: 'Creates disposable image and external-video files and deletes all returned file IDs.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'media-file-update-fabricated-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-update-fabricated-validation.ts',
    purpose:
      'Files API fileUpdate input-validation branches for long alt, source URL syntax, and originalSource length limits.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-update-fabricated-validation.json`,
      'config/parity-specs/media/media-file-update-fabricated-validation.json',
      'config/parity-requests/media/media-file-update-fabricated-validation-create.graphql',
      'config/parity-requests/media/media-file-update-validation-branches-update.graphql',
    ],
    cleanupBehavior: 'Creates one disposable image file and deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'media-file-update-source-semantics',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-update-source-semantics.ts',
    purpose:
      'Files API fileUpdate originalSource success semantics for READY MediaImage preview swaps and GenericFile direct source updates.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-update-source-semantics.json`,
      'config/parity-specs/media/media-file-update-source-semantics.json',
      'config/parity-requests/media/media-file-update-source-semantics-update.graphql',
      'config/parity-requests/media/media-file-update-source-semantics-read.graphql',
      'config/parity-requests/media/media-file-update-source-semantics-generic-read.graphql',
    ],
    cleanupBehavior: 'Creates disposable image and generic files and deletes all returned file IDs during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'media-file-update-simultaneous-source-conflict',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-update-simultaneous-source-conflict.ts',
    purpose:
      'Files API fileUpdate validation when originalSource and previewImageSource are both supplied in one or more inputs.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-update-simultaneous-source-conflict.json`,
      'config/parity-specs/media/file_update_simultaneous_source/simultaneous-source-conflict.json',
      'config/parity-requests/media/file_update_simultaneous_source/update.graphql',
    ],
    cleanupBehavior: 'Creates two disposable image files and deletes all returned file IDs during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'media-file-user-error-aggregation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-user-error-aggregation-conformance.ts',
    purpose:
      'Files API aggregate userError shape for multi-id fileDelete and fileUpdate misses plus mixed fileAcknowledgeUpdateFailed validation.',
    requiredAuthScopes: ['read_files', 'write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-user-error-aggregation.json`,
      'config/parity-specs/media/media-file-user-error-aggregation.json',
      'config/parity-specs/media/media-file-user-error-root-dispatch-validation.json',
      'config/parity-requests/media/media-file-user-error-aggregation-create.graphql',
      'config/parity-requests/media/media-file-user-error-aggregation-delete.graphql',
      'config/parity-requests/media/media-file-user-error-aggregation-update.graphql',
      'config/parity-requests/media/media-file-user-error-aggregation-acknowledge.graphql',
      'config/parity-requests/media/media-file-user-error-root-dispatch-create.graphql',
      'config/parity-requests/media/media-file-user-error-root-dispatch-update.graphql',
      'config/parity-requests/media/media-file-user-error-root-dispatch-acknowledge.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable non-ready files for acknowledge validation and deletes them in best-effort cleanup.',
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
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-create-validation-branches.json`,
      'config/parity-specs/media/media-file-create-root-dispatch-validation.json',
      'config/parity-requests/media/media-file-create-root-dispatch-validation.graphql',
    ],
    cleanupBehavior: 'Deletes any file successfully created by the acceptance branch.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'file-create-input-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-file-create-input-validation-conformance.mts',
    purpose:
      'fileCreate FileCreateInput originalSource input-class validation for missing, empty, and 2049-character values.',
    requiredAuthScopes: ['write_files'],
    fixtureOutputs: [`${CAPTURE_ROOT}media-file-create-input-validation.json`],
    cleanupBehavior: 'Validation-only requests are rejected before creating Shopify files.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'media-file-create-batch-size-limit',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-create-batch-size-limit-conformance.ts',
    purpose: 'fileCreate files input max-batch-size validation for 251 FileCreateInput entries.',
    requiredAuthScopes: ['write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-create-batch-size-limit.json`,
      'config/parity-specs/media/media-file-create-batch-size-limit.json',
      'config/parity-requests/media/media-file-create-batch-size-limit.graphql',
    ],
    cleanupBehavior: 'Validation-only request is rejected before creating Shopify files.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'media-file-create-large-batch-timestamps',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-create-large-batch-timestamps-conformance.ts',
    purpose: 'fileCreate timestamp shape for a successful 60-file batch crossing the one-minute boundary.',
    requiredAuthScopes: ['write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-create-large-batch-timestamps.json`,
      'config/parity-specs/media/media-file-create-large-batch-timestamps.json',
      'config/parity-requests/media/media-file-create-large-batch-timestamps.graphql',
    ],
    cleanupBehavior: 'Creates 60 disposable image files and deletes them during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'media-file-create-references-authorization',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-media-file-references-authorization-conformance.ts',
    purpose:
      'fileUpdate referencesToAdd product authorization denial with a short-lived delegate token lacking product write permission.',
    requiredAuthScopes: [
      'read_files',
      'write_files',
      'read_products',
      'write_products',
      'delegate access token creation',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-create-references-authorization.json`,
      'config/parity-specs/media/media-file-create-references-authorization.json',
      'config/parity-requests/media/media-file-create-references-authorization.graphql',
    ],
    cleanupBehavior:
      'Creates a disposable product and file, issues a reduced-scope delegate-token request, destroys the delegate token, deletes the file, and deletes the product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Public Admin GraphQL 2026-04 does not expose referencesToAdd on FileCreateInput, so this live capture anchors fileUpdate reference authorization while local runtime tests cover the same manage-products affordance on fileCreate.',
  },
  {
    domain: 'files',
    captureId: 'file-create-validation-precedence',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-file-create-validation-precedence-conformance.ts',
    purpose:
      'fileCreate per-input validation precedence for multi-fault source URL, filename extension, and duplicate-resolution-mode inputs.',
    requiredAuthScopes: ['write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-file-create-validation-precedence.json`,
      'config/parity-specs/media/file_create_validation_precedence/media-file-create-validation-precedence.json',
      'config/parity-requests/media/media-file-create-validation-precedence.graphql',
    ],
    cleanupBehavior: 'Deletes the disposable file created by the clean baseline branch.',
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
    captureId: 'staged-upload-default-method',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-staged-upload-default-method-conformance.ts',
    purpose:
      'stagedUploadsCreate default httpMethod target metadata for IMAGE and FILE resources when the input omits httpMethod.',
    requiredAuthScopes: ['write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-staged-uploads-create-default-http-method.json`,
      'config/parity-specs/media/media-staged-uploads-create-default-http-method.json',
      'config/parity-requests/media/media-staged-uploads-create-default-http-method.graphql',
    ],
    cleanupBehavior: 'Requests signed upload metadata only; does not upload bytes and creates no Shopify files.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'staged-upload-non-merchandising-targets',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-staged-upload-non-merchandising-conformance.ts',
    purpose:
      'stagedUploadsCreate target metadata for non-merchandising resources, PUT-vs-POST parameter shape, access-scope blocked SHOP_IMAGE, and schema-invalid import resources.',
    requiredAuthScopes: ['write_files', 'bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}media-staged-uploads-create-non-merchandising.json`,
      'config/parity-specs/media/media-staged-uploads-create-non-merchandising.json',
      'config/parity-requests/media/media-staged-uploads-create-non-merchandising.graphql',
    ],
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
      'config/parity-specs/media/media-staged-uploads-create-root-dispatch-validation.json',
      'config/parity-requests/media/media-staged-uploads-create-root-dispatch-validation.graphql',
    ],
    cleanupBehavior: 'Requests signed upload metadata only; does not upload bytes and creates no Shopify files.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'files',
    captureId: 'staged-upload-required-args',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-staged-upload-required-args-conformance.ts',
    purpose:
      'stagedUploadsCreate top-level schema coercion when required StagedUploadInput filename or mimeType is omitted.',
    requiredAuthScopes: ['write_files'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}staged_uploads_create_required_args.json`,
      'config/parity-specs/media/staged_uploads_create_required_args.json',
      'config/parity-requests/media/staged_uploads_create_required_args_missing_filename.graphql',
      'config/parity-requests/media/staged_uploads_create_required_args_missing_mime_type.graphql',
    ],
    cleanupBehavior: 'Requests validation-only staged upload metadata and creates no Shopify files.',
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
      'config/parity-specs/products/productOptionsCreate-parity-plan.json',
      'config/parity-specs/products/productOptionUpdate-parity-plan.json',
      'config/parity-specs/products/productOptionsDelete-parity-plan.json',
      'config/parity-requests/products/product-option-lifecycle-hydrate-nodes.graphql',
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
    captureId: 'product-options-reorder-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-product-options-reorder-validation-conformance.mts',
    purpose: 'productOptionsReorder validation codes, option-value reorder, and downstream read-after-write behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/product-options-reorder-validation.json',
      'config/parity-specs/products/productOptionsReorder-validation.json',
      'config/parity-requests/products/productOptionsReorder-validation.graphql',
    ],
    cleanupBehavior: 'Creates a disposable product/options/variants and deletes the product in best-effort cleanup.',
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
    captureId: 'product-variants-bulk-create-omitted-strategy',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-product-variants-bulk-create-omitted-strategy-conformance.mts',
    purpose:
      'productVariantsBulkCreate omitted strategy defaulting on a product with only Shopify standalone default variant.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}productVariantsBulkCreate-omitted-strategy-default-standalone.json`,
      'config/parity-specs/products/productVariantsBulkCreate-omitted-strategy-default-standalone.json',
      'config/parity-requests/products/productVariantsBulkCreate-omitted-strategy.graphql',
    ],
    cleanupBehavior:
      'Creates a disposable product, records omitted-strategy bulk variant create behavior, and deletes the product in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-variant-mutations',
    scriptPath: 'scripts/capture-product-variant-mutation-conformance.mts',
    purpose: 'Product variant create/update/delete mutation family.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      'config/parity-specs/products/productVariantsBulkCreate-parity-plan.json',
      'config/parity-specs/products/productVariantsBulkUpdate-parity-plan.json',
      `${CAPTURE_ROOT}product-variants-bulk-update-parity.json`,
      `${CAPTURE_ROOT}product-variants-bulk-create-parity.json`,
      `${CAPTURE_ROOT}product-variants-bulk-delete-parity.json`,
      'config/parity-specs/products/productVariantCreate-parity-plan.json',
      'config/parity-specs/products/productVariantUpdate-parity-plan.json',
      'config/parity-specs/products/productVariantDelete-parity-plan.json',
      'config/parity-specs/products/productVariantsBulkDelete-parity-plan.json',
      'config/parity-requests/products/productVariantCompatibility-setup-product.graphql',
      'config/parity-requests/products/productVariantCompatibility-setup-variant.graphql',
      'config/parity-requests/products/productVariantsBulkDelete-parity-plan.graphql',
    ],
    cleanupBehavior: 'Creates disposable products/variants and deletes the products in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-variant-delete-position-compaction',
    scriptPath: 'scripts/capture-product-variant-delete-position-compaction-conformance.mts',
    purpose:
      'Post-delete product variant position compaction for single-root compatibility and multi-id bulk delete readbacks.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-variant-delete-position-compaction.json`,
      'config/parity-specs/products/product-variant-delete-position-compaction.json',
      'config/parity-requests/products/product-variant-position-compaction-create.graphql',
      'config/parity-requests/products/product-variant-position-compaction-bulk-create.graphql',
      'config/parity-requests/products/product-variant-position-compaction-single-delete.graphql',
      'config/parity-requests/products/product-variant-position-compaction-bulk-delete.graphql',
      'config/parity-requests/products/product-variant-position-compaction-read.graphql',
    ],
    cleanupBehavior: 'Creates disposable products/options/variants and deletes products in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The current live 2025-01 schema does not expose productVariantDelete, so the single-root proxy target is compared against equivalent one-id productVariantsBulkDelete janitor evidence.',
  },
  {
    domain: 'products',
    captureId: 'product-variants-bulk-reorder-validation-resequence',
    scriptPath: 'scripts/capture-product-variants-bulk-reorder-conformance.ts',
    purpose:
      'productVariantsBulkReorder invalid position, duplicate variant id, unknown variant validation, and successful three-variant resequencing/readback.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-variants-bulk-reorder-validation-resequence.json`,
      'config/parity-specs/products/productVariantsBulkReorder-validation-resequence.json',
      'config/parity-requests/products/productVariantsBulkReorder-validation-resequence.graphql',
      'config/parity-requests/products/productVariantsBulkReorder-position-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product with a Color option and three variants, records rejected reorder branches, records successful reorder position branches and downstream reads, then deletes the product in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-variant-validations',
    scriptPath: 'scripts/capture-product-variant-validation-conformance.mts',
    purpose:
      'Bulk variant validation atomicity for create/update/delete, including public-schema options-key rejection.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_inventory'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-variants-bulk-validation-atomicity.json`,
      'config/parity-specs/products/product-variants-bulk-validation-atomicity.json',
      'config/parity-requests/products/productVariantsBulkUpdate-validation-options.graphql',
    ],
    cleanupBehavior: 'Creates disposable products and removes them after validation probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-variants-bulk-update-allow-partial',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-product-variants-bulk-update-partial-conformance.ts',
    purpose:
      'productVariantsBulkUpdate allowPartialUpdates persistence for valid inputs, read-after-write, and field/code userError ordering.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-variants-bulk-update-allow-partial-and-error-order.json`,
      'config/parity-specs/products/productVariantsBulkUpdate-allow-partial-and-error-order.json',
      'config/parity-requests/products/productVariantsBulkUpdate-allow-partial.graphql',
      'config/parity-requests/products/productVariantsBulkUpdate-allow-partial-downstream-read.graphql',
      'config/parity-requests/products/productVariantsBulkUpdate-error-order.graphql',
    ],
    cleanupBehavior:
      'Creates a disposable product with two option-backed variants, records partial update and validation probes, then deletes the product in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-variant-scalar-validations',
    scriptPath: 'scripts/capture-product-variant-scalar-validation-conformance.ts',
    purpose:
      'productVariantsBulkCreate scalar, option, and inventory validation for price, compareAtPrice, weight, per-quantity bounds, cumulative inventoryQuantities count, per-variant inventory location count, SKU, barcode, option value length, option input conflicts, duplicate option tuples, and max input size.',
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
      `${LOCAL_RUNTIME_ROOT}inventory-shipment-lifecycle-local-staging.json`,
      `${LOCAL_RUNTIME_ROOT}inventory-shipment-partial-receive-update-delete-local-staging.json`,
      `${LOCAL_RUNTIME_ROOT}inventory-shipment-validation-local-runtime.json`,
      `${LOCAL_RUNTIME_ROOT}inventory-transfer-ready-item-adjustments-local-staging.json`,
      `${CAPTURE_ROOT}inventory-transfer-create-validation.json`,
      `${CAPTURE_ROOT}inventory-transfer-lifecycle-local-staging.json`,
      'config/parity-specs/products/inventory_transfer_create_validation.json',
      'config/parity-specs/products/inventory-transfer-lifecycle-local-staging.json',
      'config/parity-requests/products/inventory-transfer-create-validation.graphql',
      'config/parity-requests/products/inventory-transfer-create.graphql',
      'config/parity-requests/products/inventory-transfer-edit.graphql',
      'config/parity-requests/products/inventory-transfer-set-items.graphql',
      'config/parity-requests/products/inventory-transfer-duplicate.graphql',
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
      'config/parity-specs/products/inventory_activate_validation.json',
      'config/parity-specs/products/inventory-idempotency-directive-lifecycle-2026-04.json',
    ],
    cleanupBehavior: 'Creates disposable products; some success paths require a second safe location before capture.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'inventory',
    captureId: 'inventory-deactivate-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-inventory-deactivate-validation-conformance.mts',
    purpose:
      'inventoryDeactivate validation for 2026-04 non-zero committed/incoming/reserved quantities, missing inventory levels, only-location errors, inventoryActivate available conflicts, and activate/deactivate generic userError code-selection schema errors.',
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
    domain: 'inventory',
    captureId: 'inventory-activate-on-hand',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-inventory-activate-on-hand-conformance.ts',
    purpose:
      'inventoryActivate onHand fresh seeding, downstream on_hand read-after-write, available/onHand conflict, already-active onHand rejection, and out-of-range onHand rejection.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_inventory', 'write_inventory', 'read_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-activate-on-hand.json`,
      'config/parity-specs/products/inventory-activate-on-hand.json',
      'config/parity-requests/products/inventory-activate-on-hand-setup.graphql',
      'config/parity-requests/products/inventory-activate-on-hand.graphql',
      'config/parity-requests/products/inventory-activate-available-on-hand-conflict.graphql',
      'config/parity-requests/products/inventory-activate-on-hand-read.graphql',
    ],
    cleanupBehavior: 'Creates disposable tracked products, records activation validation branches, then deletes them.',
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
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-invalid-compare-digest-parity.json',
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
      'config/parity-specs/products/metafieldsSet-missing-namespace.json',
      'config/parity-specs/products/metafieldsSet-owner-expansion.json',
      'config/parity-specs/products/metafieldsSet-parity-plan.json',
      'config/parity-specs/metafield-definitions/metafield-delete-not-found.json',
      'config/parity-specs/products/metafieldsSet-invalid-compare-digest.json',
      'config/parity-specs/products/metafieldsSet-missing-namespace.json',
      'config/parity-specs/products/metafieldsSet-owner-expansion.json',
      'config/parity-specs/products/metafieldsSet-parity-plan.json',
      `${CAPTURE_ROOT}metafields-delete-parity.json`,
    ],
    cleanupBehavior: 'Creates disposable products/collections and removes them after metafield probes.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'product-metafields-set-non-cas',
    scriptPath: 'scripts/capture-product-metafields-set-non-cas-conformance.mts',
    purpose:
      'Independent product-scoped metafieldsSet set/read, omitted namespace, and owner-expansion parity without selecting opaque compareDigest CAS tokens.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-missing-namespace-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-owner-expansion-parity.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-parity.json',
      'config/parity-specs/products/metafieldsSet-missing-namespace.json',
      'config/parity-specs/products/metafieldsSet-owner-expansion.json',
      'config/parity-specs/products/metafieldsSet-parity-plan.json',
      'config/parity-requests/products/metafieldsSet-downstream-read-no-compare-digest.graphql',
      'config/parity-requests/products/metafieldsSet-owner-expansion-downstream-read-no-compare-digest.graphql',
      'config/parity-requests/products/metafieldsSet-owner-expansion-no-compare-digest.graphql',
      'config/parity-requests/products/metafieldsSet-parity-plan-no-compare-digest.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product for each product-owned scenario plus one disposable collection for owner expansion; deletes all created owners after recording.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'metafields-generic-product-owner',
    scriptPath: 'scripts/capture-metafields-generic-product-owner-conformance.ts',
    purpose:
      'Product-owner metafieldsSet/metafieldsDelete live payloads and readbacks using ordinary operation names and a disposable non-fixture product owner.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafields-generic-product-owner.json`,
      'config/parity-specs/products/metafields-generic-product-owner.json',
      'config/parity-requests/products/metafieldsSet-generic-product-owner.graphql',
      'config/parity-requests/products/metafieldsDelete-generic-product-owner.graphql',
      'config/parity-requests/products/metafields-generic-product-owner-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product, records metafieldsSet/read/metafieldsDelete/read behavior, then deletes the product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafields-custom-namespace-typed-keys',
    scriptPath: 'scripts/capture-metafields-custom-namespace-typed-keys-conformance.mts',
    purpose:
      'Product-owned metafieldsSet and product read-back for custom namespace keys whose names match custom-data field types.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafieldsSet-custom-namespace-typed-keys.json`,
      'config/parity-specs/metafields/metafieldsSet-custom-namespace-typed-keys.json',
      'config/parity-requests/metafields/metafieldsSet-custom-namespace-typed-keys.graphql',
      'config/parity-requests/metafields/metafieldsSet-custom-namespace-typed-keys-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product, records custom json/rating/money metafieldsSet and read-back behavior, then deletes the product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafields-set-type-from-definition',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafields-set-type-from-definition-conformance.mts',
    purpose:
      'Product-owned metafieldDefinitionCreate followed by metafieldsSet with omitted type, proving Shopify derives the metafield type from the matching definition and downstream product reads preserve it.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafieldsSet-type-from-definition.json`,
      'config/parity-specs/metafields/metafieldsSet-type-from-definition.json',
      'config/parity-requests/metafields/metafieldsSet-type-from-definition.graphql',
      'config/parity-requests/metafields/metafieldsSet-type-from-definition-read.graphql',
    ],
    cleanupBehavior: 'Creates one disposable product and one PRODUCT metafield definition, then deletes both.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'product-metafield-owner-isolation',
    scriptPath: 'scripts/capture-product-metafield-owner-isolation-conformance.mts',
    purpose: 'Product owner-scoped metafield read isolation after staging metafields on a different product owner.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/metafields-set-owner-isolation-parity.json',
      'config/parity-specs/products/metafieldsSet-owner-isolation.json',
      'config/parity-requests/products/metafieldsSet-owner-isolation.graphql',
      'config/parity-requests/products/metafieldsSet-owner-isolation-empty-owner-read.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable products, sets a metafield on one product, captures the other product owner empty metafield read, then deletes both products.',
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
    captureId: 'metafield-definition-validations-input',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-metafield-definition-validations-input-conformance.ts',
    purpose:
      'metafieldDefinitionCreate validations[] option validation and metafieldDefinitionUpdate metaobject_definition_id immutability.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-validations-input.json`,
      'config/parity-specs/metafield-definitions/metafield-definition-validations-input.json',
      'config/parity-requests/metafield-definitions/metafield-definition-validations-input.graphql',
    ],
    cleanupBehavior:
      'Creates disposable metaobject definitions to supply valid metaobject_definition_id options, records validation branches, deletes the staged metafield definition, then deletes the metaobject definitions.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafields-set-validation-gaps',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-metafields-set-validation-gaps-conformance.ts',
    purpose:
      'metafieldsSet definition-backed value validation branches, accepted date_time offset values, and non-product ownerType payloads.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_metaobjects',
      'write_metaobjects',
      'read_content',
      'write_content',
      'read_locations',
      'read_markets',
      'write_markets',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafields-set-validation-gaps.json`,
      'config/parity-specs/metafields/metafields-set-validation-gaps.json',
      'config/parity-requests/metafields/metafields-set-definition-validation-create-definitions.graphql',
      'config/parity-requests/metafields/metafields-set-definition-validation-metaobject-definitions.graphql',
      'config/parity-requests/metafields/metafields-set-definition-validation-metaobject-create.graphql',
      'config/parity-requests/metafields/metafields-set-definition-validation-reference-definition.graphql',
      'config/parity-requests/metafields/metafields-set-list-scalar-category.graphql',
      'config/parity-requests/metafields/metafields-set-definition-validation-set.graphql',
      'config/parity-requests/metafields/metafields-set-non-product-owner-types.graphql',
    ],
    cleanupBehavior:
      'Creates a disposable product, metafield definitions, metaobject definitions, metaobject, page, blog/article, and optionally a market; deletes created definitions/resources and owner metafields during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-access-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-access-validation-conformance.ts',
    purpose:
      'metafieldDefinitionCreate/update and standardMetafieldDefinitionEnable access input validation for grants, merchant admin access, and reserved standard namespaces.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-access-validation.json`,
      'config/parity-specs/metafield-definitions/access-validation.json',
      'config/parity-requests/metafield-definitions/access-validation-create.graphql',
      'config/parity-requests/metafield-definitions/access-validation-update.graphql',
      'config/parity-requests/metafield-definitions/access-validation-standard-enable.graphql',
      'config/parity-requests/metafield-definitions/access-validation-inline-grants.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product-owned definition for update validation, records validation branches, then deletes the setup definition.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The public 2026-04 schema rejects access.grants as an unknown MetafieldAccessInput field before resolver execution; resolver userErrors cover merchant admin access and reserved standard namespace access controls.',
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-validation-job',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-validation-job-conformance.mts',
    purpose:
      'metafieldDefinitionUpdate validation backfill Job payload, job(id:) readback, and null validationJob for a subsequent non-validation update.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-validation-job.json`,
      'config/parity-specs/metafield-definitions/validation-job.json',
      'config/parity-requests/metafield-definitions/validation-job-create.graphql',
      'config/parity-requests/metafield-definitions/validation-job-metafields-set.graphql',
      'config/parity-requests/metafield-definitions/validation-job-update.graphql',
      'config/parity-requests/metafield-definitions/validation-job-read.graphql',
      'config/parity-requests/metafield-definitions/validation-job-rename.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product and product-owned metafield definition, stages a matching metafield, captures validation update/readback/rename behavior, then deletes the definition and product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-validation-affects-values',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-validation-affects-values-conformance.mts',
    purpose:
      'metafieldDefinitionUpdate validations changing later metafieldsSet value acceptance/rejection and downstream product metafield readback.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-validation-affects-values.json`,
      'config/parity-specs/metafield-definitions/validation-affects-values.json',
      'config/parity-requests/metafield-definitions/validation-affects-values-create.graphql',
      'config/parity-requests/metafield-definitions/validation-affects-values-update.graphql',
      'config/parity-requests/metafield-definitions/validation-affects-values-set.graphql',
      'config/parity-requests/metafield-definitions/validation-affects-values-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product and product-owned metafield definition, writes before and after a validation update, then deletes the definition and product.',
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
      'config/parity-requests/metafields/metafield-definition-pin-limit-and-constraint-guard.graphql',
      'config/parity-requests/metafields/metafield-definition-pin-limit-listing.graphql',
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
    captureId: 'standard-metafield-definition-enable-reenable-idempotent',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-standard-metafield-definition-enable-reenable-idempotent.mts',
    purpose:
      'standardMetafieldDefinitionEnable idempotent re-enable behavior, including id stability and over-cap pin re-enable suppression.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}standard-metafield-definition-enable-reenable-idempotent.json`,
      'config/parity-specs/metafields/standard-metafield-definition-enable-reenable-idempotent.json',
      'config/parity-requests/metafields/standard-metafield-definition-enable-reenable-idempotent.graphql',
      'config/parity-requests/metafields/standard-metafield-definition-enable-reenable-read.graphql',
    ],
    cleanupBehavior:
      'Temporarily unpins existing product definitions, creates disposable product-owned definitions to reach the pin cap, deletes them, deletes the standard definition only when the capture created it, then restores original pins.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'standard-metafield-definition-enable-material',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-standard-metafield-definition-enable-material.mts',
    purpose: 'standardMetafieldDefinitionEnable success payload for the PRODUCT shopify.material standard template.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}standard-metafield-definition-enable-material.json`,
      'config/parity-specs/metafields/standard-metafield-definition-enable-material.json',
      'config/parity-requests/metafields/standard-metafield-definition-enable-material.graphql',
    ],
    cleanupBehavior:
      'Re-enables the standard product material definition and records the idempotent success payload; the conformance shop may retain the standard definition because Shopify denied delete access for this protected standard definition.',
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
    captureId: 'metafield-definition-protected-guards',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-protected-guards-conformance.mts',
    purpose:
      'metafieldDefinitionUpdate standard-template immutable field guard and metafieldDefinitionDelete app-reserved namespace orphaned-metafields guard.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-protected-guards.json`,
      'config/parity-specs/metafields/metafield-definition-protected-guards.json',
      'config/parity-requests/metafields/metafield-definition-protected-guards-delete-no-flag.graphql',
      'config/parity-requests/metafields/metafield-definition-protected-guards-delete-with-flag.graphql',
      'config/parity-requests/metafields/metafield-definition-protected-guards-read.graphql',
      'config/parity-requests/metafields/metafield-definition-protected-guards-standard-enable.graphql',
      'config/parity-requests/metafields/metafield-definition-protected-guards-standard-update.graphql',
    ],
    cleanupBehavior:
      'Enables the standard product subtitle definition, creates a disposable product and app-reserved product definition/value, deletes the app definition with deleteAllAssociatedMetafields, deletes the product, and deletes the standard definition only when this capture created it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-delete-type-guard-no-metafields',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-delete-type-guard-no-metafields-conformance.mts',
    purpose:
      'metafieldDefinitionDelete id/reference type guards for product-owned definitions with no associated metafields.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-delete-type-guard-no-metafields.json`,
      'config/parity-specs/metafields/metafield-definition-delete-type-guard-no-metafields.json',
      'config/parity-requests/metafields/metafield-definition-delete-type-guard-no-metafields-create.graphql',
      'config/parity-requests/metafields/metafield-definition-delete-type-guard-no-metafields-delete-no-flag.graphql',
      'config/parity-requests/metafields/metafield-definition-delete-type-guard-no-metafields-delete-with-flag.graphql',
    ],
    cleanupBehavior:
      'Creates disposable product-owned id and reference definitions without setting metafields, captures guarded deletes, then deletes any remaining definitions with deleteAllAssociatedMetafields.',
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
      'config/parity-specs/metafield-definition-update/empty-constraint-values.json',
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
    captureId: 'metafield-definition-update-pin',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-update-pin.mts',
    purpose:
      'metafieldDefinitionUpdate pin/unpin handling, constrained-definition pin guard, and product-owner pin limit validation.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-update-pin.json`,
      'config/parity-specs/metafield-definitions/update-pin.json',
      'config/parity-requests/metafield-definitions/update-pin.graphql',
      'config/parity-requests/metafield-definitions/update-pin-read.graphql',
    ],
    cleanupBehavior:
      'Temporarily unpins existing product definitions, creates disposable product-owned definitions, deletes them, then restores original pins.',
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
    domain: 'metafields',
    captureId: 'metafields-set-delete-app-namespace-resolution',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafields-set-delete-app-namespace-resolution-conformance.mts',
    purpose:
      'metafieldsSet and metafieldsDelete app namespace resolution for value mutations, omitted namespace defaulting, and cross-app access denial.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafields-set-delete-app-namespace-resolution.json`,
      'config/parity-specs/metafield-definitions/metafields-set-delete-app-namespace-resolution.json',
      'config/parity-requests/metafield-definitions/metafields-set-app-namespace-resolution.graphql',
      'config/parity-requests/metafield-definitions/metafields-delete-app-namespace-resolution.graphql',
      'config/parity-requests/metafield-definitions/metafields-app-namespace-product-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product, writes app-owned metafields, deletes the product-owned app-prefixed metafield through metafieldsDelete, and deletes the product during cleanup.',
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
    captureId: 'product-duplicate-status',
    scriptPath: 'scripts/capture-product-duplicate-status-conformance.mts',
    purpose: 'productDuplicate status inheritance from the source product and explicit newStatus overrides.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}product-duplicate-status-parity.json`,
      'config/parity-specs/products/productDuplicate-status-inheritance-and-newStatus.json',
      'config/parity-requests/products/productDuplicate-status-source-create.graphql',
      'config/parity-requests/products/productDuplicate-status-no-newStatus.graphql',
      'config/parity-requests/products/productDuplicate-status-newStatus.graphql',
      'config/parity-requests/products/productDuplicate-status-read.graphql',
    ],
    cleanupBehavior:
      'Creates ACTIVE and DRAFT disposable source products, duplicates each source, records downstream duplicate reads, and deletes all four products during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'product-duplicate-async',
    scriptPath: 'scripts/capture-product-duplicate-async-conformance.ts',
    purpose: 'Asynchronous productDuplicate operation success and missing-product completion behavior.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      'config/parity-specs/products/productDuplicate-parity-plan.json',
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
    captureId: 'saved-search-default-record-update-delete',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-default-record-update-delete-conformance.ts',
    purpose: 'SavedSearch update/delete behavior for persisted ORDER and DRAFT_ORDER default saved-search records.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_draft_orders', 'write_draft_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-default-record-update-delete.json`,
      'config/parity-specs/saved-searches/saved-search-default-record-update-delete.json',
      'config/parity-requests/saved-searches/saved-search-default-records-read.graphql',
      'config/parity-requests/saved-searches/saved-search-default-record-update-order.graphql',
      'config/parity-requests/saved-searches/saved-search-default-record-read-updated-order.graphql',
      'config/parity-requests/saved-searches/saved-search-default-record-delete-draft-order.graphql',
      'config/parity-requests/saved-searches/saved-search-default-record-read-deleted-draft-order.graphql',
    ],
    cleanupBehavior:
      'Restores the updated ORDER default fields and recreates the deleted DRAFT_ORDER default by name/query/resourceType.',
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
    captureId: 'saved-search-app-namespace',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-app-namespace-conformance.ts',
    purpose:
      'SavedSearch create/update query-input $app metafield namespace resolution before staging, payload serialization, and downstream saved-search reads.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-app-namespace.json`,
      'config/parity-specs/saved-searches/saved-search-app-namespace.json',
      'config/parity-requests/saved-searches/saved-search-app-namespace-read.graphql',
      'config/parity-requests/saved-searches/saved-search-app-namespace-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product saved search with a $app metafield query, reads it, updates it with a second $app metafield query, reads it again, then deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-filter-projection',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-filter-projection-conformance.ts',
    purpose:
      'SavedSearch filters projection and filters.__typename for range comparators, exists syntax, bounded ranges, and negated range terms.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-filter-projection.json`,
      'config/parity-specs/saved-searches/saved-search-filter-projection.json',
      'config/parity-requests/saved-searches/saved-search-filter-projection-create.graphql',
      'config/parity-requests/saved-searches/saved-search-filter-projection-delete.graphql',
      'config/parity-requests/saved-searches/saved-search-filter-projection-read-after-create.graphql',
    ],
    cleanupBehavior: 'Creates five disposable product saved searches and deletes each one during cleanup.',
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
      `${CAPTURE_ROOT}saved-search-incompatible-filter-aggregation.json`,
      'config/parity-specs/saved-searches/saved-search-incompatible-filter-aggregation.json',
      'config/parity-requests/saved-searches/saved-search-incompatible-filter-aggregation-create.graphql',
      'config/parity-specs/saved-searches/saved-search-query-grammar-validation.json',
      'config/parity-specs/saved-searches/saved-search-reserved-filter-update-field.json',
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
      'SavedSearch per-resource unknown-filter validation for PRODUCT create/update plus known-filter positive control.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-unknown-filter-field.json`,
      'config/parity-specs/saved-searches/saved-search-unknown-filter-field.json',
      'config/parity-requests/saved-searches/saved-search-unknown-filter-field.graphql',
      'config/parity-requests/saved-searches/saved-search-unknown-filter-field-update.graphql',
    ],
    cleanupBehavior: 'Creates one disposable product saved search for positive-control validation and deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-multi-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-multi-validation-conformance.ts',
    purpose:
      'SavedSearch create/update aggregate independent name, query, reserved-filter, unknown-filter, and incompatible-filter validation userErrors.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-multi-validation.json`,
      'config/parity-specs/saved-searches/saved-search-multi-validation.json',
      'config/parity-requests/saved-searches/saved-search-multi-validation-create.graphql',
      'config/parity-requests/saved-searches/saved-search-multi-validation-update.graphql',
    ],
    cleanupBehavior: 'Creates two disposable product saved searches and deletes both during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-blank-name-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-blank-name-validation-conformance.ts',
    purpose:
      'savedSearchCreate empty-name validation returns schema-shaped UserError field/message payloads and aggregates query validation errors.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-blank-name-validation.json`,
      'config/parity-specs/saved-searches/saved-search-blank-name-validation.json',
      'config/parity-requests/saved-searches/saved-search-blank-name-validation-create.graphql',
    ],
    cleanupBehavior: 'No Shopify writes are committed because both savedSearchCreate aliases fail validation.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'saved-searches',
    captureId: 'saved-search-blank-name-update',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-saved-search-blank-name-update-conformance.ts',
    purpose:
      'savedSearchUpdate explicitly supplied empty-name validation returns field/message UserErrors and leaves downstream reads unchanged.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-blank-name-update.json`,
      'config/parity-specs/saved-searches/saved-search-blank-name-update.json',
      'config/parity-requests/saved-searches/saved-search-blank-name-update-create.graphql',
      'config/parity-requests/saved-searches/saved-search-blank-name-update.graphql',
      'config/parity-requests/saved-searches/saved-search-blank-name-update-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product saved search and deletes it after the rejected blank-name update/read capture.',
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
      'savedSearchCreate and savedSearchUpdate required input coercion, including variable-supplied create input, plus explicit empty-query create success.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}saved-search-required-input-validation.json`,
      'config/parity-specs/saved-searches/saved-search-required-input-validation.json',
      'config/parity-requests/saved-searches/saved-search-required-input-empty-query-create.graphql',
      'config/parity-requests/saved-searches/saved-search-required-input-missing-id-update.graphql',
      'config/parity-requests/saved-searches/saved-search-required-input-missing-name-create.graphql',
      'config/parity-requests/saved-searches/saved-search-required-input-missing-resource-type-create.graphql',
      'config/parity-requests/saved-searches/saved-search-required-input-variable-missing-name-create.graphql',
      'config/parity-requests/saved-searches/saved-search-required-input-variable-missing-resource-type-create.graphql',
      'config/parity-specs/saved-searches/saved-search-required-input-variable-validation.json',
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
      'config/parity-specs/products/selling-plan-product-variant-associations.json',
      'config/parity-requests/products/product-options-hydrate-nodes.graphql',
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
      'productVariantAppendMedia and productVariantDetachMedia validation for pair-count, media-count, duplicate-variant, invalid-media-type, cross-product, non-ready, already-attached, and unattached detach targets.',
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
      'config/parity-requests/products/selling-plan-group-add-products.graphql',
      'config/parity-requests/products/selling-plan-group-add-variants.graphql',
      'config/parity-requests/products/selling-plan-group-catalog.graphql',
      'config/parity-requests/products/selling-plan-group-create.graphql',
      'config/parity-requests/products/selling-plan-group-read.graphql',
      'config/parity-requests/products/selling-plan-group-update.graphql',
    ],
    cleanupBehavior: 'Creates a disposable product and selling-plan group, then deletes both during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'selling-plan-group-update-delete-to-zero',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-selling-plan-group-update-delete-to-zero-conformance.ts',
    purpose:
      'sellingPlanGroupUpdate rejects deleting the only existing selling plan without a replacement, leaves readback unchanged, and allows delete-with-replacement.',
    requiredAuthScopes: ['read_products', 'write_products', 'write_purchase_options'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}selling-plan-group-update-delete-to-zero.json`,
      'config/parity-specs/products/selling-plan-group-update-delete-to-zero.json',
      'config/parity-requests/products/selling-plan-group-update-delete-to-zero-create.graphql',
      'config/parity-requests/products/selling-plan-group-update-delete-to-zero-update.graphql',
      'config/parity-requests/products/selling-plan-group-update-delete-to-zero-read.graphql',
    ],
    cleanupBehavior: 'Creates one disposable selling-plan group, captures update/readback branches, then deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'products',
    captureId: 'selling-plan-group-input-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-selling-plan-group-input-validation-conformance.ts',
    purpose:
      'Selling-plan group create/update input validation for group limits, nested selling-plan guardrails, pricing-policy fixed-policy requirements, recurring billing cycle ranges, and recurring delivery cutoff ranges.',
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
    domain: 'selling-plans',
    captureId: 'selling-plan-group-create-active-model-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-selling-plan-group-create-active-model-validation-conformance.ts',
    purpose:
      'Selling-plan group create model-backed validation for blank names, create-only plan count bounds, missing plan policies, and update empty-create-list carve-out.',
    requiredAuthScopes: ['read_products', 'write_products', 'write_purchase_options'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}selling-plan-group-create-active-model-validation.json`,
      'config/parity-specs/selling-plans/sellingPlanGroupCreate-active-model-validation.json',
      'config/parity-requests/selling-plans/sellingPlanGroupCreate-active-model-validation.graphql',
      'config/parity-requests/selling-plans/sellingPlanGroupUpdate-empty-create-list.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable selling-plan group, verifies empty create-list update, then deletes the group.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'selling-plans',
    captureId: 'selling-plan-group-summary',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-selling-plan-group-summary-conformance.ts',
    purpose:
      'SellingPlanGroup.summary computed field after create/readback, including plan count, frequency pluralization, percentage range, fixed-value range, and mixed discount pieces.',
    requiredAuthScopes: ['read_products', 'write_products', 'write_purchase_options'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}selling-plan-group-summary.json`,
      'config/parity-specs/selling-plans/sellingPlanGroup-summary.json',
      'config/parity-requests/selling-plans/sellingPlanGroupShopCurrency.graphql',
      'config/parity-requests/selling-plans/sellingPlanGroupCreate-summary.graphql',
      'config/parity-requests/selling-plans/sellingPlanGroupSummary-read.graphql',
    ],
    cleanupBehavior: 'Creates one disposable selling-plan group, reads it back, then deletes the group.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'selling-plans',
    captureId: 'selling-plan-group-app-id-readback',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-selling-plan-group-app-id-readback-conformance.ts',
    purpose:
      'Selling-plan group create and update appId persistence, including read-after-write and explicit null clearing.',
    requiredAuthScopes: ['read_products', 'write_products', 'write_purchase_options'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}selling-plan-group-app-id-readback.json`,
      'config/parity-specs/selling-plans/sellingPlanGroup-app-id-readback.json',
      'config/parity-requests/selling-plans/sellingPlanGroupCreate-app-id-readback.graphql',
      'config/parity-requests/selling-plans/sellingPlanGroupRead-app-id-readback.graphql',
      'config/parity-requests/selling-plans/sellingPlanGroupUpdate-app-id-readback.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable selling-plan group, reads appId after create, updates and clears appId, then deletes the group.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-mutations',
    scriptPath: 'scripts/capture-metafield-definition-mutation-conformance.mts',
    purpose:
      'standardMetafieldDefinitionEnable validation branches plus regular metafieldDefinition create/update/delete/pin/unpin userError typename evidence.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}standard-metafield-definition-enable-validation.json`,
      'config/parity-specs/metafields/standard-metafield-definition-enable-validation.json',
      'config/parity-requests/metafields/standard-metafield-definition-enable-validation.graphql',
      'config/parity-requests/metafields/metafield-definition-user-error-typenames.graphql',
    ],
    cleanupBehavior:
      'Records standard enable validation branches, then creates disposable product/reference setup for regular metafieldDefinition userError typename probes and deletes created definitions/products during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metafields',
    captureId: 'metafield-definition-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metafield-definition-lifecycle-conformance.mts',
    purpose:
      'Product-owned metafieldDefinitionCreate/update/delete lifecycle plus Metafield.definition association on metafieldsSet payloads and owner reads.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-lifecycle-mutations.json`,
      'config/parity-specs/metafields/metafield-definition-lifecycle-mutations.json',
      'config/parity-requests/metafields/metafield-definition-lifecycle-metafields-set.graphql',
      'config/parity-requests/metafields/metafield-definition-lifecycle-read-product-metafield.graphql',
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
    captureId: 'metafield-definition-owner-scoped-duplicates',
    scriptPath: 'scripts/capture-metafield-definition-owner-scoped-duplicates.mts',
    purpose:
      'metafieldDefinitionCreate duplicate TAKEN behavior and owner-type scoped namespace/key coexistence for PRODUCT and CUSTOMER definitions.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metafield-definition-owner-scoped-duplicates.json`,
      'config/parity-specs/metafields/metafield-definition-owner-scoped-duplicates.json',
      'config/parity-requests/metafields/metafield-definition-owner-scoped-create.graphql',
      'config/parity-requests/metafields/metafield-definition-owner-scoped-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable PRODUCT and CUSTOMER metafield definitions sharing a namespace/key, captures duplicate PRODUCT TAKEN and owner-type readback, then deletes both definitions.',
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
    captureId: 'metaobject-delete-not-found',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-delete-not-found-conformance.mts',
    purpose:
      'Metaobject delete fabricated-id not-found payload, including deletedId null and RECORD_NOT_FOUND userError shape.',
    requiredAuthScopes: ['write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-delete-not-found.json`,
      'config/parity-specs/metaobjects/metaobject-delete-not-found.json',
    ],
    cleanupBehavior: 'No setup; sends metaobjectDelete with a fabricated id and does not mutate live resources.',
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
    captureId: 'metaobject-name-independence-create',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-name-independence-conformance.ts',
    purpose:
      'metaobjectCreate payload and read-after-write behavior when the client operation name is CreateMetaobject instead of the captured lifecycle fixture name.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-name-independence-create.json`,
      'config/parity-specs/metaobjects/metaobject-name-independence-create.json',
      'config/parity-requests/metaobjects/metaobject-name-independence-definition-create.graphql',
      'config/parity-requests/metaobjects/metaobject-name-independence-create.graphql',
      'config/parity-requests/metaobjects/metaobject-name-independence-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable metaobject definition and one row, records normal-name create/read behavior, then deletes the row and definition.',
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
      'Metaobject definition create fieldDefinitions validation for reserved keys, duplicate input, displayNameKey resolution, invalid field types, hyphen keys, and max field count.',
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
    captureId: 'metaobject-definition-customer-account-access',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-customer-account-access-conformance.ts',
    purpose:
      'Metaobject definition access.customerAccount READ/NONE create/update persistence, read-after-write projection, and invalid enum coercion.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-definition-customer-account-access.json`,
      'config/parity-specs/metaobjects/metaobject-definition-customer-account-access.json',
      'config/parity-requests/metaobjects/metaobject-definition-customer-account-access-create.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-customer-account-access-update.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-customer-account-access-read.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-customer-account-access-invalid-create.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-customer-account-access-invalid-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable READ and NONE access definitions, captures update/readback branches, captures invalid enum validation-only branches, then deletes created definitions.',
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
    captureId: 'metaobject-definition-field-key-min-length',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-field-key-min-length-conformance.ts',
    purpose:
      'Metaobject definition create/update fieldDefinitions key length and character validation for single-character keys, empty keys, 2/64-character accepted boundaries, 65-character rejection, mixed-case acceptance, and invalid format errors.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobjectDefinition-field-key-min-length.json`,
      'config/parity-specs/metaobjects/metaobjectDefinition-field-key-min-length.json',
      'config/parity-requests/metaobjects/metaobjectDefinition-field-key-min-length-create.graphql',
      'config/parity-requests/metaobjects/metaobjectDefinition-field-key-min-length-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable setup and boundary definitions; deletes successful definitions during cleanup. Validation branches create no records.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-definition-field-operation-errors',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-field-operation-errors-conformance.ts',
    purpose:
      'metaobjectDefinitionUpdate field-operation conflict plus create-key validation userError codes, field paths, messages, and multi-conflict ordering.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobjectDefinitionUpdate-field-operation-errors.json`,
      'config/parity-specs/metaobjects/metaobjectDefinitionUpdate-field-operation-errors.json',
      'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-field-operation-errors-create.graphql',
      'config/parity-requests/metaobjects/metaobjectDefinitionUpdate-field-operation-errors-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable metaobject definition, captures validation-only field operation conflicts, then deletes the definition.',
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
    captureId: 'metaobject-definition-lifecycle-invariants',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-lifecycle-invariants-conformance.ts',
    purpose:
      'metaobjectDefinitionCreate reserved standard-template and shopify-- namespace validation plus live discovery for unavailable app-managed/dependent-on-app delete guard candidates.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-definition-lifecycle-invariants.json`,
      'config/parity-specs/metaobjects/metaobject-definition-lifecycle-invariants.json',
      'config/parity-requests/metaobjects/metaobject-definition-lifecycle-invariants-create.graphql',
    ],
    cleanupBehavior:
      'Runs validation-only create probes and read-only schema/catalog discovery; no records are created and no cleanup is expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Delete guard branches remain runtime-test-backed until a conformance credential can reach an app-config-managed definition and an app-dependent standard definition.',
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
    captureId: 'metaobject-definition-recreate-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-recreate-conformance.ts',
    purpose:
      'Metaobject definition delete and recreate with the same type/name, proving old fields/rows are not retained and post-recreate rows use the new field set.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-definition-recreate-lifecycle.json`,
      'config/parity-specs/metaobjects/metaobject-definition-recreate-lifecycle.json',
      'config/parity-requests/metaobjects/metaobject-definition-recreate-entry-create.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-recreate-post-delete-read.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-recreate-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable definition and row, deletes that definition during the scenario, recreates the same type/name with a different field set and two rows, then best-effort deletes remaining rows/definition during cleanup.',
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
      'config/parity-specs/metaobjects/metaobject-create-duplicate-field-input.json',
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
    captureId: 'metaobject-display-name-conflict',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-display-name-conflict-conformance.ts',
    purpose:
      'metaobjectUpdate/metaobjectUpsert DISPLAY_NAME_CONFLICT when a display-name change collides with another same-type metaobject row used as a linked product option value.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-display-name-conflict.json`,
      'config/parity-specs/metaobjects/metaobject-display-name-conflict.json',
      'config/parity-requests/metaobjects/metaobject-display-name-conflict-definition-create.graphql',
      'config/parity-requests/metaobjects/metaobject-display-name-conflict-entry-create.graphql',
      'config/parity-requests/metaobjects/metaobject-display-name-conflict-metafield-definition-create.graphql',
      'config/parity-requests/metaobjects/metaobject-display-name-conflict-product-create.graphql',
      'config/parity-requests/metaobjects/metaobject-display-name-conflict-product-options-create.graphql',
      'config/parity-requests/metaobjects/metaobject-display-name-conflict-update.graphql',
      'config/parity-requests/metaobjects/metaobject-display-name-conflict-upsert.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable metaobject definition, two rows, one product metafield definition, and one product with linked option values; captures update/upsert conflict responses, then deletes the product, metafield definition, rows, and metaobject definition.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-update-redirect-new-handle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-update-redirect-new-handle-conformance.ts',
    purpose:
      'metaobjectUpdate redirectNewHandle behavior for online-store renderable metaobjects, explicit false input, and non-renderable no-op handling.',
    requiredAuthScopes: [
      'read_metaobjects',
      'write_metaobjects',
      'read_online_store_navigation',
      'write_online_store_navigation',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-update-redirect-new-handle.json`,
      'config/parity-specs/metaobjects/metaobject-update-redirect-new-handle.json',
      'config/parity-requests/metaobjects/metaobject-update-redirect-new-handle-definition-create.graphql',
      'config/parity-requests/metaobjects/metaobject-update-redirect-new-handle-entry-create.graphql',
      'config/parity-requests/metaobjects/metaobject-update-redirect-new-handle-update.graphql',
      'config/parity-requests/metaobjects/metaobject-update-redirect-new-handle-url-redirect.graphql',
      'config/parity-requests/metaobjects/metaobject-update-redirect-new-handle-url-redirects.graphql',
    ],
    cleanupBehavior:
      'Creates disposable metaobject definitions and rows for each branch, deletes any redirect row observed from the redirect-true branch, then deletes rows and definitions.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-definition-update-url-handle-redirect',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-definition-update-url-handle-redirect-conformance.ts',
    purpose:
      'metaobjectDefinitionUpdate onlineStore.data.urlHandle change with createRedirects true, two published row redirects, URL redirect downstream reads, and canCreateRedirects definition readback.',
    requiredAuthScopes: [
      'read_metaobjects',
      'write_metaobjects',
      'read_online_store_navigation',
      'write_online_store_navigation',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobjectDefinitionUpdate-url-handle-redirect.json`,
      'config/parity-specs/metaobjects/metaobjectDefinitionUpdate-url-handle-redirect.json',
      'config/parity-requests/metaobjects/metaobject-definition-update-url-handle-redirect-definition-create.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-update-url-handle-redirect-entry-create.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-update-url-handle-redirect-update.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-update-url-handle-redirect-definition-read.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-update-url-handle-redirect-url-redirect.graphql',
      'config/parity-requests/metaobjects/metaobject-definition-update-url-handle-redirect-url-redirects.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable online-store metaobject definition and two ACTIVE rows, updates the definition URL handle with createRedirects true, deletes observed URL redirects, then deletes rows and the definition.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'metaobjects',
    captureId: 'metaobject-online-store-template-suffix',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-online-store-template-suffix-conformance.ts',
    purpose:
      'Entry-level onlineStore.templateSuffix create, update preservation, explicit update, downstream readback, and upsert create/update behavior.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-online-store-template-suffix.json`,
      'config/parity-specs/metaobjects/metaobject-online-store-template-suffix.json',
      'config/parity-requests/metaobjects/metaobject-online-store-template-suffix-definition-create.graphql',
      'config/parity-requests/metaobjects/metaobject-online-store-template-suffix-create.graphql',
      'config/parity-requests/metaobjects/metaobject-online-store-template-suffix-update.graphql',
      'config/parity-requests/metaobjects/metaobject-online-store-template-suffix-upsert.graphql',
      'config/parity-requests/metaobjects/metaobject-online-store-template-suffix-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable online-store metaobject definition and several rows, captures templateSuffix lifecycle behavior, then deletes rows and the definition.',
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
    captureId: 'metaobject-mutation-arg-shape',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-mutation-arg-shape-conformance.ts',
    purpose:
      'Public GraphQL schema-validation evidence for metaobjectDefinitionUpdate resetFieldOrder and standardMetaobjectDefinitionEnable enabledByShopify argument visibility.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-mutation-arg-shape.json`,
      'config/parity-specs/metaobjects/metaobject-mutation-arg-shape.json',
      'config/parity-requests/metaobjects/metaobject-mutation-arg-shape-reset.graphql',
      'config/parity-requests/metaobjects/metaobject-mutation-arg-shape-public-enable.graphql',
    ],
    cleanupBehavior:
      'Runs validation-only GraphQL requests that fail before resolver execution; no records are created and no cleanup is expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Internal enabledByShopify success evidence remains unavailable to the public conformance credential and is covered by focused runtime tests only.',
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
    captureId: 'metaobject-auto-handle-generation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-metaobject-auto-handle-generation-conformance.ts',
    purpose:
      'metaobjectCreate generated handle and fallback displayName shape when no handle is supplied and the definition has no displayNameKey, plus explicit mixed-case handle lowercasing, titleized display-name fallback, case-insensitive conflict suffixing, and metaobjectUpsert create/update fallback displayName derivation from the explicit handle.',
    requiredAuthScopes: ['read_metaobjects', 'write_metaobjects'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}metaobject-auto-handle-generation.json`,
      'config/parity-specs/metaobjects/metaobject-auto-handle-generation.json',
      'config/parity-requests/metaobjects/metaobject-auto-handle-generation-create.graphql',
      'config/parity-requests/metaobjects/metaobject-auto-handle-generation-definition-create.graphql',
      'config/parity-requests/metaobjects/metaobject-auto-handle-generation-upsert.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable metaobject definition and five rows, then deletes the rows and definition during cleanup.',
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
      'config/parity-specs/products/inventoryAdjustQuantities-parity-plan.json',
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
    captureId: 'inventory-move-adjustment-group-shape',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-inventory-move-adjustment-group-shape-conformance.ts',
    purpose:
      'inventoryMoveQuantities InventoryAdjustmentGroup id and createdAt payload shape, after a public inventorySetQuantities setup mutation.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-move-adjustment-group-shape.json`,
      'config/parity-specs/products/inventory-move-adjustment-group-shape.json',
      'config/parity-requests/products/inventory-move-adjustment-group-shape-set.graphql',
      'config/parity-requests/products/inventory-move-adjustment-group-shape.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable tracked product, captures set-plus-move inventory mutations at an active location, then deletes the product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-quantity-updated-at-and-after-change-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-inventory-quantity-updated-at-and-after-change-local-runtime.ts',
    purpose:
      'Executable local-runtime parity for staged inventory quantity updatedAt and quantityAfterChange read-after-write behavior.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}inventory-quantity-updated-at-and-after-change-local-runtime.json`,
      'config/parity-specs/products/inventory-quantity-updated-at-and-after-change-local-runtime.json',
      'config/parity-requests/products/inventory-quantity-updated-at-and-after-change-local-runtime.graphql',
    ],
    cleanupBehavior:
      'Local-runtime set and move scenario only; proxy reset during parity replay clears staged inventory and no Shopify cleanup is required.',
    expectedStatusChecks: ['targeted-runtime-test', 'conformance:parity', 'conformance:check', 'rust:test'],
    notes:
      'This complements older live inventory captures that recorded null quantityAfterChange by guarding the supported draft-proxy staging contract without runtime Shopify writes.',
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
    captureId: 'inventorysetquantities-quantity-bounds',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-inventory-set-quantities-bounds-conformance.ts',
    purpose:
      'inventorySetQuantities negative and lower-bound quantity validation against the 2026-04 @idempotent/changeFromQuantity contract.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventorySetQuantities-quantity-bounds.json`,
      'config/parity-specs/products/inventorySetQuantities-quantity-bounds.json',
    ],
    cleanupBehavior: 'Creates one disposable product per quantity-bound branch and deletes each product immediately.',
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
      'config/parity-requests/products/inventory-quantity-contracts-2026-downstream-read.graphql',
      'config/parity-requests/products/inventory-quantity-contracts-2026-set-unknown-id-validation.graphql',
      'config/parity-requests/products/inventory-quantity-contracts-2026-set-on-hand.graphql',
      'config/parity-requests/products/inventory-quantity-contracts-2026-set-on-hand-validation.graphql',
      'config/parity-requests/products/inventory-quantity-contracts-2026-set-on-hand-missing-idempotency.graphql',
      'config/parity-requests/products/inventory-quantity-contracts-2026-missing-set-on-hand-change-from.graphql',
      'config/parity-requests/products/inventory-quantity-contracts-2026-adjust-unknown-id-validation.graphql',
      'config/parity-requests/products/inventory-quantity-contracts-2026-move-unknown-id-validation.graphql',
      `${CAPTURE_ROOT}inventory-quantity-contracts-2026-04.json`,
      'config/parity-specs/products/inventory-quantity-contracts-2026-04.json',
      'config/parity-specs/products/inventory-quantity-idempotency-directive-2026-04.json',
    ],
    cleanupBehavior: 'Creates one disposable product, records set/adjust quantity contract branches, then deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-reason-validation-2026',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-inventory-reason-validation-conformance.ts',
    purpose:
      'Inventory quantity mutation adjustment reason validation against Shopify public adjustment reasons on 2026-04.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-reason-validation.json`,
      'config/parity-specs/products/inventory-reason-validation.json',
      'config/parity-requests/products/inventory-reason-validation-setup.graphql',
      'config/parity-requests/products/inventory-reason-validation-set.graphql',
      'config/parity-requests/products/inventory-reason-validation-adjust.graphql',
      'config/parity-requests/products/inventory-reason-validation-move.graphql',
      'config/parity-requests/products/inventory-reason-validation-set-on-hand.graphql',
      'config/parity-requests/products/inventory-reason-validation-downstream.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product, records accepted and invalid inventory reason branches, then deletes the product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-adjust-zero-delta-noop',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-inventory-adjust-zero-delta-noop-conformance.ts',
    purpose:
      'inventoryAdjustQuantities all-zero no-op payload shape plus mixed zero/non-zero adjustment-group behavior.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-adjust-zero-delta-noop.json`,
      'config/parity-specs/products/inventory-adjust-zero-delta-noop.json',
      'config/parity-requests/products/inventory-adjust-zero-delta-noop.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable tracked products, records all-zero and mixed zero/non-zero adjust calls, then deletes both products.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-adjust-ledger-document-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-inventory-adjust-ledger-document-validation-conformance.ts',
    purpose:
      'inventoryAdjustQuantities ledgerDocumentUri validation for required, forbidden, internal-gid, max-one-document, and valid non-available branches.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-adjust-ledger-document-validation.json`,
      'config/parity-specs/products/inventory-adjust-ledger-document-validation.json',
      'config/parity-requests/products/inventory-adjust-ledger-document-validation.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable tracked products, records ledger document validation branches and one valid incoming adjustment, then deletes both products.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'inventory',
    captureId: 'inventory-adjust-name-allowlist',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-inventory-adjust-name-allowlist-conformance.ts',
    purpose:
      'inventoryAdjustQuantities and inventoryMoveQuantities public quantity-name allowlist validation plus deprecated set-on-hand acceptance evidence.',
    requiredAuthScopes: ['read_inventory', 'write_inventory', 'read_locations', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}inventory-adjust-name-allowlist.json`,
      'config/parity-specs/products/inventory-adjust-name-allowlist.json',
      'config/parity-requests/products/inventory-adjust-name-allowlist-adjust.graphql',
      'config/parity-requests/products/inventory-adjust-name-allowlist-move.graphql',
      'config/parity-requests/products/inventory-adjust-name-allowlist-set-on-hand.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable tracked product, records invalid adjust/move branches and a deprecated set-on-hand success branch, then deletes the product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-conformance.mts',
    purpose:
      'Shop locale lifecycle, primary/missing-locale validation guards, and translation read-after-write cleanup behavior.',
    requiredAuthScopes: [
      'read_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
      'read_markets',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-disable-clears-translations.json`,
      `${CAPTURE_ROOT}localization-shop-locale-primary-guards.json`,
      'config/parity-specs/localization/localization-disable-clears-translations.json',
      'config/parity-specs/localization/localization-shop-locale-primary-guards.json',
    ],
    cleanupBehavior:
      'Enables the French shop locale, registers one product-title translation, disables the locale, captures validation-only primary/missing-locale guard branches, and leaves the locale/translation state cleaned up.',
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
    captureId: 'localization-shop-locale-usererror-no-code',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-shop-locale-usererror-no-code-conformance.mts',
    purpose:
      'shopLocaleEnable, shopLocaleUpdate, and shopLocaleDisable reject userErrors.code selection because their payloads expose plain UserError.',
    requiredAuthScopes: ['read_locales', 'write_locales'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-shop-locale-usererror-no-code.json`,
      'config/parity-specs/localization/localization-shop-locale-usererror-no-code.json',
      'config/parity-requests/localization/localization-shop-locale-usererror-no-code.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture; selecting userErrors.code fails before shop-locale resolvers run, so no locale state is mutated.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-translations-digest-mismatch',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-translations-digest-mismatch-conformance.mts',
    purpose: 'translationsRegister current digest validation for correct, fabricated, and stale product title digests.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-translations-digest-mismatch.json`,
      'config/parity-specs/localization/localization-translations-digest-mismatch.json',
      'config/parity-requests/localization/localization-translations-digest-product-create.graphql',
      'config/parity-requests/localization/localization-translations-digest-product-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable product, enables French only when needed, captures correct/wrong/stale digest branches, deletes the product, and restores the locale when the script enabled it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-translation-updated-at',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-translations-digest-mismatch-conformance.mts',
    purpose:
      'translationsRegister Translation.updatedAt payload presence and downstream translatableResource readback for a known Product resource.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-translation-updated-at.json`,
      'config/parity-specs/localization/localization-translation-updated-at.json',
      'config/parity-requests/localization/localization-translation-updated-at-register.graphql',
      'config/parity-requests/localization/localization-translation-updated-at-read.graphql',
    ],
    cleanupBehavior:
      'Reuses the digest-mismatch known-product setup, registers a French title translation selecting updatedAt, reads it back with updatedAt, then deletes the disposable product and restores the locale when the script enabled it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-translations-invalid-key',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-translations-invalid-key-conformance.mts',
    purpose:
      'translationsRegister Product key validation for one valid title row plus one invalid-key row in the same request.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-translations-invalid-key.json`,
      'config/parity-specs/localization/localization-translations-invalid-key.json',
    ],
    cleanupBehavior:
      'Creates one disposable product, enables French only when needed, captures invalid-key partial success and downstream readback, deletes the product, and restores the locale when the script enabled it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-handle-translation-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-handle-translation-validation-conformance.mts',
    purpose:
      'translationsRegister handle translation normalization, too-long validation, and downstream read behavior.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-handle-translation-validation.json`,
      'config/parity-specs/localization/localization-handle-translation-validation.json',
    ],
    cleanupBehavior:
      'Creates one disposable product, enables French only when needed, captures normalized and too-long handle translation branches, deletes the product, and restores the locale when the script enabled it.',
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
      'translationsRemove unknown-key and disabled-locale no-op success behavior plus translationsRegister disabled-locale and primary-locale validation.',
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
      'Creates one disposable product, temporarily enables and disables Italian for the disabled-locale removal/register branches, deletes the product, and restores Italian only if it was enabled before capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-translations-validation-order',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-translations-validation-order-conformance.mts',
    purpose:
      'translationsRegister first-error precedence for rows that violate locale and blank-value validation at the same time.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
    ],
    fixtureOutputs: [
      'config/parity-requests/localization/localization-translations-known-resource-product-create.graphql',
      'config/parity-requests/localization/localization-translations-validation-order-read.graphql',
      `${CAPTURE_ROOT}localization-translations-validation-order.json`,
      'config/parity-specs/localization/localization-translations-validation-order.json',
    ],
    cleanupBehavior:
      'Creates one disposable product, disables Italian for the validation capture when needed, captures non-enabled and primary-locale first-error branches, deletes the product, and restores Italian only if it was enabled before capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-translatable-content-product',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-translatable-content-conformance.mts',
    purpose:
      'Product translatableResource.translatableContent key/value/digest/locale/type read parity for fully populated source fields.',
    requiredAuthScopes: ['read_products', 'write_products', 'read_translations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-translatable-content-product.json`,
      'config/parity-specs/localization/localization-translatable-content-product.json',
      'config/parity-requests/localization/localization-translatable-content-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable Product with title, body HTML, handle, product type, and SEO source fields populated, validates translatableContent digests, then deletes the Product.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-market-translations',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-market-translations-conformance.mts',
    purpose: 'Market-scoped translationsRegister/translationsRemove product-title/product_type lifecycle.',
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
      'Creates one disposable product, enables Spanish only when needed, registers/removes two market-scoped translations in one remove call, deletes the product, and restores the locale when the script enabled it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-translations-value-matches-original',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-translations-value-matches-original-conformance.mts',
    purpose:
      'translationsRegister market-scoped VALUE_MATCHES_ORIGINAL_CONTENT validation, digest precedence, downstream no-stage behavior, and accepted sibling branches.',
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
      `${CAPTURE_ROOT}localization-translations-value-matches-original.json`,
      'config/parity-specs/localization/localization-translations-value-matches-original.json',
    ],
    cleanupBehavior:
      'Creates one disposable product, enables Spanish only when needed, captures market-scoped value-matches-original rejection and accepted sibling rows, deletes the product, and restores the locale when the script enabled it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-collection-translations',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-collection-translations-conformance.mts',
    purpose: 'Collection translationsRegister/translationsRemove lifecycle and downstream translatable-resource reads.',
    requiredAuthScopes: [
      'read_products',
      'write_products',
      'read_translations',
      'write_translations',
      'read_locales',
      'write_locales',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-collection-translation-lifecycle.json`,
      'config/parity-specs/localization/localization-collection-translation-lifecycle.json',
      'config/parity-requests/localization/localization-collection-translation-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable custom Collection, enables French only when needed, registers/removes one Collection title translation, deletes the Collection, and restores the locale when the script enabled it.',
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
    domain: 'localization',
    captureId: 'localization-shop-locale-market-web-presence-filter',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-shop-locale-market-web-presence-filter-conformance.mts',
    purpose: 'ShopLocale enable/update silently filter marketWebPresenceIds to WebPresences owned by the shop.',
    requiredAuthScopes: ['read_markets', 'read_locales', 'write_locales'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-shop-locale-market-web-presence-filter.json`,
      'config/parity-specs/localization/localization-shop-locale-market-web-presence-filter.json',
      'config/parity-requests/localization/localization-shop-locale-market-web-presence-filter-setup.graphql',
      'config/parity-requests/localization/localization-shop-locale-market-web-presence-filter-read.graphql',
      'config/parity-requests/localization/localization-shop-locale-market-web-presence-filter-update.graphql',
    ],
    cleanupBehavior:
      'Disables French before capture when already enabled, enables French with one valid and one fabricated MarketWebPresence ID, updates the same locale with the mixed ID set, then disables French.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-shop-locale-web-presence-sync',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-shop-locale-web-presence-sync-conformance.mts',
    purpose:
      'ShopLocale enable/update marketWebPresenceIds synchronously add and remove WebPresence alternateLocales and derived rootUrls.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_locales', 'write_locales'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-shop-locale-enable-web-presence-sync.json`,
      `${CAPTURE_ROOT}localization-shop-locale-update-web-presence-sync.json`,
      'config/parity-specs/localization/localization-shop-locale-enable-web-presence-sync.json',
      'config/parity-specs/localization/localization-shop-locale-update-web-presence-sync.json',
      'config/parity-requests/localization/localization-shop-locale-web-presence-sync-enable.graphql',
      'config/parity-requests/localization/localization-shop-locale-web-presence-sync-read.graphql',
      'config/parity-requests/localization/localization-shop-locale-web-presence-sync-update.graphql',
    ],
    cleanupBehavior:
      'Disables French before each scenario, creates disposable WebPresences, records enable and update read-after-write behavior, disables French, deletes the disposable WebPresences, then restores the pre-capture French locale if it existed.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'localization',
    captureId: 'localization-translations-unknown-resource',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-localization-translations-unknown-resource-conformance.mts',
    purpose:
      'translationsRegister and translationsRemove for an unknown/non-existent resource ID return NOT_FOUND userErrors without staging any translations.',
    requiredAuthScopes: ['read_products', 'read_translations', 'write_translations', 'read_locales', 'write_locales'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}localization-translations-unknown-resource.json`,
      'config/parity-specs/localization/localization-translations-unknown-resource.json',
      'config/parity-requests/localization/localization-translations-unknown-resource.graphql',
      'config/parity-requests/localization/localization-unknown-resource-validation.graphql',
    ],
    cleanupBehavior:
      'Creates a disposable product, uses a non-existent GID for unknown-resource validation, then deletes the disposable product during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'markets',
    scriptPath: 'scripts/capture-market-conformance.mts',
    purpose: 'Markets read baselines and localization-adjacent validation probes.',
    requiredAuthScopes: ['read_markets', 'read_products'],
    fixtureOutputs: [
      'config/parity-requests/markets/price-list-catalog-validation-markets-read.graphql',
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
      'Safe Markets lifecycle validation branches for blank and too-short marketCreate input plus unknown marketUpdate/marketDelete IDs.',
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
    captureId: 'market-unsupported-country-region',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-unsupported-country-region-conformance.mts',
    purpose:
      'marketCreate UNSUPPORTED_COUNTRY_REGION validation and Shopify-derived unsupported MarketRegionCreateInput country code list.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-create-unsupported-country-region.json`,
      'src/proxy/market_unsupported_country_regions.rs',
      'config/parity-specs/markets/market-create-unsupported-country-region.json',
      'config/parity-requests/markets/market-create-unsupported-country-region.graphql',
    ],
    cleanupBehavior:
      'Primary and per-country probe mutations reject before changing market state; no setup or cleanup records are created.',
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
    captureId: 'price-list-catalog-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-price-list-catalog-validation-conformance.ts',
    purpose:
      'priceListCreate and priceListUpdate catalogId validation for nonexistent catalogs and catalogs that already have a price list assigned.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}price-list-catalog-validation.json`,
      'config/parity-specs/markets/price-list-create-catalog-does-not-exist.json',
      'config/parity-specs/markets/price-list-create-catalog-wrong-gid-type.json',
      'config/parity-specs/markets/price-list-create-catalog-taken.json',
      'config/parity-specs/markets/price-list-update-catalog-does-not-exist.json',
      'config/parity-specs/markets/price-list-update-catalog-wrong-gid-type.json',
      'config/parity-specs/markets/price-list-update-catalog-taken.json',
      'config/parity-requests/markets/price-list-input-validation-markets-read.graphql',
      'config/parity-requests/markets/price-list-create-catalog-validation.graphql',
      'config/parity-requests/markets/price-list-update-input-validation.graphql',
      'config/parity-requests/markets/catalog-create-relation-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable price lists and a market catalog for taken-branch setup, records validation failures that do not create or update records, then deletes all created price lists and catalogs.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Uses a never-created MarketCatalog gid for CATALOG_DOES_NOT_EXIST, a wrong-resource CatalogMarket gid for RESOURCE_NOT_FOUND invalid-id behavior, and a disposable MarketCatalog with an attached price list for CATALOG_TAKEN.',
  },
  {
    domain: 'markets',
    captureId: 'price-list-name-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-price-list-name-validation-conformance.ts',
    purpose:
      'priceListCreate and priceListUpdate name validation for duplicate names and names longer than 255 characters.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}price-list-name-validation.json`,
      'config/parity-specs/markets/price-list-name-validation.json',
      'config/parity-requests/markets/price-list-name-validation-create.graphql',
      'config/parity-requests/markets/price-list-name-validation-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable setup price lists for duplicate/update validation, records rejected duplicate and over-length branches, then deletes all successfully created price lists.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'markets-user-error-typename',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-markets-user-error-typename-conformance.ts',
    purpose:
      'Typed userErrors __typename discriminator validation for price-list, quantity-rules, and web-presence mutation payloads.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}markets-user-error-typename.json`,
      'config/parity-specs/markets/markets-user-error-typename.json',
      'config/parity-requests/markets/markets-user-error-typename.graphql',
    ],
    cleanupBehavior:
      'Runs safe validation branches with blank input or unknown ids; no store records are created, updated, or deleted.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'price-list-update-detach-catalog',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-price-list-update-detach-catalog-conformance.ts',
    purpose:
      'priceListUpdate explicit null catalog detach behavior, empty catalogId invalid-variable validation, downstream catalog/price-list read coherence, and reattach claim behavior.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}price-list-update-detach-catalog.json`,
      'config/parity-specs/markets/price-list-update-detach-catalog.json',
      'config/parity-requests/markets/price-list-update-detach-catalog-read.graphql',
      'config/parity-requests/markets/catalog-relation-markets-read.graphql',
      'config/parity-requests/markets/catalog-create-relation-validation.graphql',
      'config/parity-requests/markets/price-list-create-catalog-validation.graphql',
      'config/parity-requests/markets/price-list-update-input-validation.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable price list, attaches it to a disposable market catalog, captures empty and null catalogId update behavior, creates a replacement catalog that claims the detached price list, then deletes created catalogs and the disposable price list when still present.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Public Admin GraphQL 2026-04 returns top-level INVALID_VARIABLE for variable catalogId: "" rather than a PriceListUserError payload.',
  },
  {
    domain: 'markets',
    captureId: 'catalog-relation-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-catalog-relation-validation-conformance.mts',
    purpose:
      'Catalog price-list/publication relation validation for unknown ids, one-catalog relation exclusivity, and freshly-created publication attachment.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_publications', 'write_publications'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}catalog-relation-validation.json`,
      'config/parity-specs/markets/catalog-create-fresh-publication-relation.json',
      'config/parity-specs/markets/catalog-create-price-list-not-found.json',
      'config/parity-specs/markets/catalog-create-price-list-taken.json',
      'config/parity-specs/markets/catalog-update-publication-not-found.json',
      'config/parity-requests/markets/catalog-create-publication-relation.graphql',
      'config/parity-requests/markets/catalog-relation-markets-read.graphql',
      'config/parity-requests/markets/catalog-relation-publication-create.graphql',
      'config/parity-requests/markets/catalog-create-relation-validation.graphql',
      'config/parity-requests/markets/catalog-update-relation-validation.graphql',
      'config/parity-requests/markets/price-list-create-catalog-validation.graphql',
    ],
    cleanupBehavior:
      'Uses an existing market context, creates a disposable price list, publication, and setup catalogs, captures rejected and accepted relation branches, then deletes the disposable catalogs, price list, and publication when still present.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'bundled-price-list-web-presence',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bundled-price-list-web-presence-conformance.mts',
    purpose:
      'Single-document priceListCreate plus webPresenceCreate payload parity for the bundled local-dispatch path.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bundled-price-list-web-presence.json`,
      'config/parity-specs/markets/bundled-price-list-web-presence-create.json',
      'config/parity-requests/markets/bundled-price-list-web-presence-create.graphql',
    ],
    cleanupBehavior:
      'Reads baseline webPresences for the local preflight cassette, creates one disposable price list and subfolder web presence in a single GraphQL document, then deletes both resources.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'catalog-create-unknown-market-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-catalog-create-unknown-market-validation-conformance.mts',
    purpose: 'Safe catalogCreate validation branch for an unknown context market id.',
    requiredAuthScopes: ['write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}catalog-create-unknown-market-validation.json`,
      'config/parity-specs/markets/catalog-create-unknown-market-validation.json',
      'config/parity-requests/markets/catalog-create-relation-validation.graphql',
    ],
    cleanupBehavior:
      'Validation-only mutation rejects before creating a catalog; no setup or cleanup records are created.',
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
    captureId: 'market-update-scalars',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-update-scalars-conformance.mts',
    purpose:
      'marketUpdate scalar name/status lifecycle, enabled coupling, and downstream market(id:) read-after-write for a disposable market.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-update-scalars.json`,
      'config/parity-specs/markets/market-update-scalars.json',
      'config/parity-requests/markets/market-update-scalars-create.graphql',
      'config/parity-requests/markets/market-update-scalars-update.graphql',
      'config/parity-requests/markets/market-update-scalars-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable active market, updates name/status to DRAFT, records payload plus readback, then deletes the market.',
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
    captureId: 'price-list-fixed-prices-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-price-list-fixed-prices-validation-conformance.mts',
    purpose:
      'Variant-level priceListFixedPricesAdd, priceListFixedPricesUpdate, and priceListFixedPricesDelete validation branches for price/compareAtPrice currency mismatch, duplicate variant IDs, unknown variants, missing fixed prices, and unknown price lists.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}price-list-fixed-prices-validation.json`,
      'config/parity-specs/markets/price-list-fixed-prices-add-compare-at-currency-mismatch.json',
      'config/parity-specs/markets/price-list-fixed-prices-add-currency-mismatch.json',
      'config/parity-specs/markets/price-list-fixed-prices-add-duplicate-variant-id.json',
      'config/parity-specs/markets/price-list-fixed-prices-add-price-list-not-found.json',
      'config/parity-specs/markets/price-list-fixed-prices-add-variant-not-found.json',
      'config/parity-specs/markets/price-list-fixed-prices-delete-price-list-not-found.json',
      'config/parity-specs/markets/price-list-fixed-prices-delete-price-not-fixed.json',
      'config/parity-specs/markets/price-list-fixed-prices-delete-variant-not-found.json',
      'config/parity-specs/markets/price-list-fixed-prices-update-compare-at-currency-mismatch.json',
      'config/parity-specs/markets/price-list-fixed-prices-update-currency-mismatch.json',
      'config/parity-specs/markets/price-list-fixed-prices-update-duplicate-variant-id.json',
      'config/parity-specs/markets/price-list-fixed-prices-update-price-list-not-found.json',
      'config/parity-specs/markets/price-list-fixed-prices-update-price-not-fixed.json',
      'config/parity-specs/markets/price-list-fixed-prices-update-variant-not-found.json',
      'config/parity-requests/markets/price-list-fixed-prices-add-validation.graphql',
      'config/parity-requests/markets/price-list-fixed-prices-delete-validation.graphql',
      'config/parity-requests/markets/price-list-fixed-prices-update-validation.graphql',
    ],
    cleanupBehavior:
      'Deletes the target variant fixed price before recording, after duplicate add, after seeded update validation, and at final cleanup; validation-only branches reject without Shopify mutations.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'market-create-handle-dedupe',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-handle-dedupe-conformance.mts',
    purpose:
      'marketCreate generated handle slug dedupe for distinct names that collide after Shopify slugification plus case-insensitive duplicate name validation.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-create-handle-dedupe.json`,
      'config/parity-specs/markets/market-create-handle-dedupe.json',
      'config/parity-requests/markets/market-create-handle-dedupe.graphql',
    ],
    cleanupBehavior:
      'Creates disposable Europe and Europe! markets, records case-insensitive duplicate-name validation and generated handle dedupe, then deletes created markets in reverse creation order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'market-create-region-node-shape',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-region-node-shape-conformance.mts',
    purpose:
      'marketCreate country region node id/name/code/__typename payload shape plus downstream market(id:) read-after-write.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-create-region-node-shape.json`,
      'config/parity-specs/markets/market-create-region-node-shape.json',
      'config/parity-requests/markets/market-create-region-node-shape.graphql',
      'config/parity-requests/markets/market-create-region-node-shape-read.graphql',
    ],
    cleanupBehavior: 'Creates one disposable Canada region market, reads it back, then deletes the created market.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'market-create-price-inclusions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-price-inclusions-conformance.mts',
    purpose:
      'marketCreate nested priceInclusions success, downstream Market.priceInclusions read-after-write, and inclusive-pricing incompatibility validation for non-region market conditions.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-create-price-inclusions.json`,
      'config/parity-specs/markets/market-create-price-inclusions.json',
      'config/parity-requests/markets/market-create-price-inclusions.graphql',
      'config/parity-requests/markets/market-price-inclusions-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable region market with explicit price inclusions, verifies read-after-write, records a rejected locations-condition branch, then deletes the created market.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'market-create-enabled-without-status',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-market-create-enabled-without-status-conformance.mts',
    purpose:
      'marketCreate accepts enabled: true when status is omitted, without returning INVALID_STATUS_AND_ENABLED_COMBINATION.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-create-enabled-without-status.json`,
      'config/parity-specs/markets/market-create-enabled-without-status.json',
      'config/parity-requests/markets/market-create-enabled-without-status.graphql',
    ],
    cleanupBehavior: 'Creates one disposable market with enabled true and omitted status, then deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'market-create-plan-limit-markets-home',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-market-create-plan-limit-conformance.mts',
    purpose: 'marketCreate plan-limit skip on a Markets Home shop when creating a fourth enabled market.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-create-plan-limit-markets-home.json`,
      'config/parity-specs/markets/market-create-plan-limit-markets-home.json',
      'config/parity-requests/markets/market-create-plan-limit.graphql',
    ],
    cleanupBehavior:
      'Creates four disposable enabled markets, asserts all four succeed without SHOP_REACHED_PLAN_MARKETS_LIMIT, then deletes created markets in reverse order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'market-create-currency-settings',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-market-currency-settings-conformance.mts',
    purpose:
      'marketCreate currencySettings localCurrencies and roundingEnabled accepted shapes, read-after-write, baseCurrencyManualRate positive-value validation, and CurrencyCode enum coercion.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-create-currency-settings.json`,
      'config/parity-specs/markets/market-create-currency-settings-euro-name.json',
      'config/parity-specs/markets/market-create-currency-settings-enum-coercion.json',
      'config/parity-specs/markets/market-create-currency-settings-flags.json',
      'config/parity-specs/markets/market-create-currency-settings-manual-rate-validation.json',
      'config/parity-specs/markets/market-create-currency-settings-xaf-base-currency.json',
      'config/parity-requests/markets/market-create-currency-settings.graphql',
      'config/parity-requests/markets/market-create-currency-settings-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable Markets Home markets with currencySettings flags and non-USD base currencies, reads them back, captures validation-only manual-rate and invalid-currency branches, then deletes created markets in reverse order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'catalog-context-update-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-catalog-context-update-conformance.ts',
    purpose:
      'catalogContextUpdate required-context validation, remove-only context updates, duplicate market add behavior, catalog-not-found typing, company-location context updates, driver mismatch validation, downstream catalog reads, and catalogsCount after catalog writes.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}catalog-context-update-lifecycle.json`,
      'config/parity-specs/markets/catalog-context-update-no-args.json',
      'config/parity-specs/markets/catalog-context-update-removes-only.json',
      'config/parity-specs/markets/catalog-context-update-market-taken.json',
      'config/parity-specs/markets/catalog-context-update-catalog-not-found.json',
      'config/parity-specs/markets/catalog-context-update-company-location-add.json',
      'config/parity-specs/markets/catalog-context-update-company-location-not-found.json',
      'config/parity-specs/markets/catalog-context-update-driver-type-mismatch.json',
      'config/parity-requests/markets/catalog-context-update-catalog-create.graphql',
      'config/parity-requests/markets/catalog-context-update-catalog-not-found.graphql',
      'config/parity-requests/markets/catalog-context-update-company-catalog-create.graphql',
      'config/parity-requests/markets/catalog-context-update-company-create.graphql',
      'config/parity-requests/markets/catalog-context-update-company-location-add-remove.graphql',
      'config/parity-requests/markets/catalog-context-update-company-location-create.graphql',
      'config/parity-requests/markets/catalog-context-update-company-read.graphql',
      'config/parity-requests/markets/catalog-context-update-driver-mismatch.graphql',
      'config/parity-requests/markets/catalog-context-update-market-create.graphql',
      'config/parity-requests/markets/catalog-context-update-market-taken.graphql',
      'config/parity-requests/markets/catalog-context-update-no-args.graphql',
      'config/parity-requests/markets/catalog-context-update-read.graphql',
      'config/parity-requests/markets/catalog-context-update-removes-only.graphql',
      'config/parity-requests/markets/catalog-context-update-unknown-id-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable markets, B2B companies, MarketCatalogs, and CompanyLocationCatalogs, records catalogContextUpdate branches, deletes catalogs in reverse creation order, deletes markets in reverse creation order, then deletes companies in reverse creation order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'markets',
    captureId: 'quantity-pricing-by-variant-update-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-quantity-pricing-by-variant-update-validation-conformance.mts',
    purpose:
      'quantityPricingByVariantUpdate validation branches for userError typename, add-side currency and duplicates, delete-side missing variants/price-break IDs, and quantity-rule numeric invariants.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}quantity-pricing-by-variant-update-validation.json`,
      'config/parity-specs/markets/quantity-pricing-by-variant-update-validation.json',
      'config/parity-requests/markets/quantity-pricing-by-variant-update-validation.graphql',
    ],
    cleanupBehavior:
      'Pre-cleans quantity pricing for the configured variant, records reject-only validation branches, records accepted delete no-op evidence, seeds one disposable fixed price/rule/price break to prove delete-by-variant cleanup, then deletes the seeded quantity pricing.',
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
      'MarketLocalizableResource default text-metafield behavior plus definition-backed money-metafield marketLocalizationsRegister/remove lifecycle parity.',
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
      'Creates one disposable draft product with a text metafield, creates a disposable money metafield definition and metafield, probes market localization behavior, deletes the definition with associated metafields, then deletes the product.',
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
      'Web presence create/update/delete lifecycle, downstream top-level webPresences reads, multi-locale rootUrls, locale catalog breadth/invalid-locale validation, duplicate subfolder-suffix validation, duplicate-language validation, non-letter subfolder-suffix validation, and primary-domain delete guard.',
    requiredAuthScopes: ['read_markets', 'write_markets'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}market-web-presence-lifecycle-parity.json`,
      'config/parity-specs/markets/web-presence-lifecycle-local-staging.json',
      'config/parity-specs/markets/web-presence-root-urls-multi-locale.json',
      'config/parity-specs/markets/web-presence-partial-update-alternate-locales.json',
      'config/parity-specs/markets/web-presence-subfolder-suffix-taken.json',
      'config/parity-specs/markets/web-presence-duplicate-languages-validation.json',
      'config/parity-specs/markets/web-presence-subfolder-suffix-non-letter.json',
      'config/parity-specs/markets/web-presence-create-italian-default-locale.json',
      'config/parity-specs/markets/web-presence-create-invalid-default-locale.json',
      'config/parity-requests/markets/web-presence-lifecycle-create.graphql',
      'config/parity-requests/markets/web-presence-suffix-market-create.graphql',
      'config/parity-requests/markets/web-presence-suffix-market-update.graphql',
      'config/parity-specs/markets/web-presence-delete-primary-blocked.json',
    ],
    cleanupBehavior:
      'Records the primary-domain webPresenceDelete guard without cleanup because it must fail, creates one disposable subfolder web presence, updates it, deletes it, records multi-locale and Italian default-locale disposable web presences, records invalid-locale, duplicate-language, and non-letter subfolder validation branches, deletes all disposable web presences, and verifies the baseline read after cleanup.',
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
    domain: 'markets',
    captureId: 'catalog-delete',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-catalog-delete-conformance.mts',
    purpose:
      'catalogDelete typed CatalogUserError shape for unknown IDs, successful delete payload shape, and captured-empty context-driver rejection exploration.',
    requiredAuthScopes: ['read_markets', 'write_markets', 'read_companies', 'write_companies'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}catalog-delete-parity.json`,
      'config/parity-specs/markets/catalog-delete-unknown-id-validation.json',
      'config/parity-specs/markets/catalog-delete-success-payload.json',
      'config/parity-requests/markets/catalog-delete-unknown-id-validation.graphql',
      'config/parity-requests/markets/catalog-delete-success-setup-read.graphql',
      'config/parity-requests/markets/catalog-delete-success.graphql',
    ],
    cleanupBehavior:
      'Records unknown-ID validation, creates and deletes a disposable MarketCatalog for the success payload, and probes MarketCatalog plus CompanyLocationCatalog deletion; any disposable catalog/company state is deleted in cleanup.',
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
    captureId: 'marketing-engagement-response-shape',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-marketing-engagement-response-shape-conformance.mts',
    purpose:
      'Marketing engagement create immediate response shape for full input, sparse input missing V2 required fields, and missing occurredOn.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-engagement-create-response-shape.json`,
      'config/parity-specs/marketing/marketing-engagement-create-response-shape.json',
      'config/parity-requests/marketing/marketing-engagement-response-shape-create-activity.graphql',
      'config/parity-requests/marketing/marketing-engagement-response-shape-full.graphql',
      'config/parity-requests/marketing/marketing-engagement-response-shape-sparse.graphql',
      'config/parity-requests/marketing/marketing-engagement-response-shape-missing-occurred-on.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable external marketing activity, captures full and omitted-field engagement create branches, then deletes the activity.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'marketing',
    captureId: 'marketing-activity-status-label',
    scriptPath: 'scripts/capture-marketing-activity-status-label-conformance.mts',
    purpose:
      'External marketing activity statusLabel derivation for ad, post, newsletter, inactive, and deleted-externally branches.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-activity-status-label.json`,
      'config/parity-specs/marketing/marketing-activity-status-label.json',
      'config/parity-requests/marketing/marketing-activity-status-label.graphql',
      'config/parity-requests/marketing/marketing-activity-status-label-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable external marketing activities, records immediate and read-after-write statusLabel strings, then deletes every disposable activity by remote ID.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'marketing',
    captureId: 'marketing-activity-create-external-default-status',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-marketing-activity-create-external-default-status-conformance.mts',
    purpose:
      'External marketing activity create omitted-status default, explicit ACTIVE statusLabel control, and upsert omitted-status public schema rejection.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-activity-create-external-default-status.json`,
      'config/parity-specs/marketing/marketing-activity-create-external-default-status.json',
      'config/parity-requests/marketing/marketing-activity-create-external-default-status.graphql',
      'config/parity-requests/marketing/marketing-activity-create-external-default-status-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable external marketing activities for omitted-status create and explicit ACTIVE create, records an omitted-status upsert-create schema rejection, reads created activities back, then deletes every disposable remote ID.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
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
    captureId: 'marketing-activity-source-and-medium',
    scriptPath: 'scripts/capture-marketing-activity-source-and-medium-conformance.mts',
    purpose:
      'External marketing activity sourceAndMedium derivation for tactic-specific labels, referring-domain aliases, and default channel fallback.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-activity-source-and-medium.json`,
      'config/parity-specs/marketing/marketing-activity-source-and-medium.json',
      'config/parity-requests/marketing/marketing-activity-source-and-medium.graphql',
      'config/parity-requests/marketing/marketing-activity-source-and-medium.variables.json',
    ],
    cleanupBehavior:
      'Deletes every deterministic disposable remote ID before and after recording the sourceAndMedium create matrix.',
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
    captureId: 'marketing-activity-update-currency-and-tactic-guards',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-marketing-activity-update-currency-and-tactic-guards-conformance.mts',
    purpose:
      'External marketing activity update and upsert currency mismatch plus STOREFRONT_APP tactic transition userErrors.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-activity-update-currency-and-tactic-guards.json`,
      'config/parity-specs/marketing/marketing-activity-update-currency-and-tactic-guards.json',
      'config/parity-requests/marketing/marketing-activity-update-currency-and-tactic-guards-read.graphql',
      'config/parity-requests/marketing/marketing-activity-update-currency-and-tactic-guards.graphql',
      'config/parity-requests/marketing/marketing-activity-update-from-storefront-guard.graphql',
    ],
    cleanupBehavior:
      'Creates disposable baseline and STOREFRONT_APP external marketing activities, captures rejected update/upsert guard branches and readbacks, then deletes remaining activities by remote ID and ID.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'marketing',
    captureId: 'marketing-activity-create-external-read-after-write',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-marketing-activity-create-external-read-after-write-conformance.mts',
    purpose:
      'External marketing activity create/update read-after-write for activity adSpend and event scheduled end preservation when update omits those fields.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-activity-create-external-read-after-write.json`,
      'config/parity-specs/marketing/marketing-activity-create-external-read-after-write.json',
      'config/parity-requests/marketing/marketing-activity-create-external-read-after-write.graphql',
      'config/parity-requests/marketing/marketing-activity-create-external-read-after-write-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable external marketing activity with adSpend, scheduledStart, scheduledEnd, and referringDomain, updates it while omitting those fields, captures readback, then deletes by remote ID and ID.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Admin GraphQL 2026-04 exposes the scheduled-end readback on nested MarketingEvent.scheduledToEndAt; MarketingActivity itself does not expose scheduled/referring-domain output fields on the current public schema.',
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
    captureId: 'marketing-activity-upsert-external-validation',
    scriptPath: 'scripts/capture-marketing-activity-upsert-external-validation-conformance.mts',
    purpose:
      'External marketing activity upsert-create validation for budget/adSpend currency mismatch and uniqueness userErrors.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-activity-upsert-external-validation.json`,
      'config/parity-specs/marketing/marketing-activity-upsert-external-validation.json',
      'config/parity-requests/marketing/marketing-activity-upsert-external-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable external marketing activities needed for upsert-create uniqueness probes, captures rejected validation branches, then deletes every disposable remote ID.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'marketing',
    captureId: 'marketing-external-activity-url-scheme-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-marketing-external-activity-url-scheme-validation-conformance.mts',
    purpose:
      'External marketing activity create/update/upsert remoteUrl and remotePreviewImageUrl URL scheme validation.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-external-activity-url-scheme-validation.json`,
      'config/parity-specs/marketing/marketing-external-activity-url-scheme-validation.json',
      'config/parity-requests/marketing/marketing-external-activity-url-scheme-create.graphql',
      'config/parity-requests/marketing/marketing-external-activity-url-scheme-update.graphql',
      'config/parity-requests/marketing/marketing-external-activity-url-scheme-upsert.graphql',
      'config/parity-requests/marketing/marketing-external-activity-url-scheme-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable valid external marketing activity for update/read probes, captures invalid URL scheme branches, then deletes every disposable remote ID.',
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
    domain: 'marketing',
    captureId: 'marketing-engagement-create-validation-order',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-marketing-engagement-create-validation-order-conformance.mts',
    purpose:
      'Marketing engagement create selector-count, channel-handle, currency, missing-activity, and invalid-channel message validation.',
    requiredAuthScopes: ['read_marketing_events', 'write_marketing_events'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}marketing-engagement-create-validation-order.json`,
      'config/parity-specs/marketing/marketing-engagement-create-invalid-channel-handle.json',
      'config/parity-specs/marketing/marketing-engagement-create-validation-order.json',
      'config/parity-requests/marketing/marketing-engagement-create-invalid-channel-handle.graphql',
      'config/parity-requests/marketing/marketing-engagement-create-validation-order-multiple-activity-selectors.graphql',
      'config/parity-requests/marketing/marketing-engagement-create-validation-order-multiple-channel-selectors.graphql',
      'config/parity-requests/marketing/marketing-engagement-create-validation-order-setup.graphql',
      'config/parity-requests/marketing/marketing-engagement-create-validation-order-unknown-channel-currency.graphql',
      'config/parity-requests/marketing/marketing-engagement-create-validation-order-unknown-remote-currency.graphql',
      'config/parity-requests/marketing/marketing-engagement-create-validation-order-unknown-remote.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable external marketing activity, captures rejected validation-order branches, then deletes the activity.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Soft-deleted MarketingEvent preservation is not recordable through the public Admin API; local runtime tests cover that deleted-event state.',
  },
  {
    domain: 'segments',
    captureId: 'segments',
    scriptPath: 'scripts/capture-segment-conformance.mts',
    purpose:
      'Segment baseline read payloads for the checked-in segment parity request. The proxy forwards this read upstream (de-seeded), so the fixture carries the forwarded upstreamCalls instead of a seed precondition.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}segments-baseline.json`,
      'config/parity-specs/segments/segments-baseline-read.json',
      'config/parity-requests/segments/segments-baseline-read.variables.json',
    ],
    cleanupBehavior:
      'Creates one disposable segment so the knownSegment detail read resolves, then deletes it after the baseline read.',
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
    captureId: 'segment-local-runtime-dispatch-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-segment-local-runtime-dispatch-validation.ts',
    purpose:
      'Local-runtime guard that segmentCreate dispatches and stages locally for neutral operation names without upstream passthrough.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}segment-local-runtime-dispatch-validation.json`,
      'config/parity-specs/segments/segment-local-runtime-dispatch-validation.json',
      'config/parity-requests/segments/segment-local-runtime-dispatch-validation.graphql',
    ],
    cleanupBehavior:
      'Local-runtime create scenario only; proxy reset during parity replay clears the synthetic segment and no Shopify cleanup is required.',
    expectedStatusChecks: ['targeted-runtime-test', 'conformance:parity', 'conformance:check', 'rust:test'],
    notes:
      'This is executable local-runtime evidence for dispatch/staging, not Shopify fidelity evidence. Live segment resolver behavior remains covered by the Shopify segment validation fixtures.',
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
    captureId: 'segment-name-suffix-and-limit',
    scriptPath: 'scripts/capture-segment-name-suffix-and-limit-conformance.ts',
    purpose:
      'segmentCreate duplicate-name suffix counters for `(0)`/`(1)` and the real 6000 segment-limit userError branch.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}segment-name-suffix-and-limit.json`,
      'config/parity-specs/segments/segment-name-suffix-and-limit.json',
      'config/parity-requests/segments/segment-create-limit-setup-chunk.graphql',
      'config/parity-requests/segments/segment-create-limit-validation.graphql',
      'config/parity-requests/segments/segment-name-suffix-duplicate.graphql',
    ],
    cleanupBehavior:
      'Clears existing segments from the disposable conformance shop, creates 6000 setup segments through public segmentCreate mutations plus four suffix probe segments, captures the overflow branch, then deletes every segment it created.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'segments',
    captureId: 'segments-user-errors-shape',
    scriptPath: 'scripts/capture-segments-user-errors-shape-conformance.ts',
    purpose:
      'segmentCreate/segmentUpdate/segmentDelete default UserError shape, segmentUpdate literal-null mutable attributes, plus customerSegmentMembersQueryCreate typed userError code and field shape.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}segments-user-errors-shape.json`,
      'config/parity-specs/segments/segments-user-errors-shape.json',
      'config/parity-requests/segments/segments-user-errors-shape-member-query-create.graphql',
      'config/parity-requests/segments/segments-user-errors-shape-segment-create.graphql',
      'config/parity-requests/segments/segments-user-errors-shape-segment-delete.graphql',
      'config/parity-requests/segments/segments-user-errors-shape-segment-update.graphql',
      'config/parity-requests/segments/segments-user-errors-shape-segment-update-name-null.graphql',
      'config/parity-requests/segments/segments-user-errors-shape-segment-update-query-null.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable segment for segmentUpdate id-only and literal-null validation branches, reads it back after null-only updates, and deletes it during cleanup; all other captured branches are validation-only.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'segments',
    captureId: 'segment-length-edge-cases',
    scriptPath: 'scripts/capture-segment-length-edge-cases-conformance.ts',
    purpose:
      'segmentCreate/segmentUpdate whitespace edge cases where name length is validated after stripping and query length is validated against raw input.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}segment-create-update-length-edge-cases.json`,
      'config/parity-specs/segments/segment-create-update-length-edge-cases.json',
      'config/parity-requests/segments/segment-create-validation-limits.graphql',
      'config/parity-requests/segments/segment-update-name-validation-limits.graphql',
      'config/parity-requests/segments/segment-update-query-validation-limits.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable segment for accepted padded-name create/update branches, records raw-query length validation failures, and deletes the disposable segment during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'segments',
    captureId: 'segment-query-whitespace-preservation',
    scriptPath: 'scripts/capture-segment-query-whitespace-preservation-conformance.ts',
    purpose:
      'segmentCreate/segmentUpdate query storage fidelity where leading and trailing query whitespace is preserved in mutation payloads and downstream segment reads.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}segment-query-whitespace-preservation.json`,
      'config/parity-specs/segments/segment-query-whitespace-preservation.json',
      'config/parity-requests/segments/segment-query-whitespace-create.graphql',
      'config/parity-requests/segments/segment-query-whitespace-read.graphql',
      'config/parity-requests/segments/segment-query-whitespace-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable segment, reads it back, updates its query with a different padded string, and deletes the disposable segment during cleanup.',
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
    captureId: 'segment-required-argument-validation',
    scriptPath: 'scripts/capture-segment-required-argument-validation-conformance.ts',
    purpose:
      'segmentCreate/segmentUpdate/segmentDelete omitted and literal-null required top-level argument GraphQL coercion envelopes.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}segment-mutations-required-argument-validation.json`,
      'config/parity-specs/segments/segment-mutations-required-argument-validation.json',
      'config/parity-requests/segments/segment-create-required-args-missing-both.graphql',
      'config/parity-requests/segments/segment-create-required-args-missing-name.graphql',
      'config/parity-requests/segments/segment-create-required-args-missing-query.graphql',
      'config/parity-requests/segments/segment-create-required-args-null-name.graphql',
      'config/parity-requests/segments/segment-create-required-args-null-query.graphql',
      'config/parity-requests/segments/segment-delete-required-id-missing.graphql',
      'config/parity-requests/segments/segment-delete-required-id-null.graphql',
      'config/parity-requests/segments/segment-update-required-id-missing.graphql',
      'config/parity-requests/segments/segment-update-required-id-null.graphql',
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
    domain: 'segments',
    captureId: 'customer-segment-members-query-create-segment-id-paths',
    scriptPath: 'scripts/capture-customer-segment-members-query-create-segment-id-paths-conformance.ts',
    purpose:
      'customerSegmentMembersQueryCreate segmentId branches for stored broad segment query grammar, unknown valid Segment GID CDP error shape, and malformed/wrong-resource Segment GID top-level coercion.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-segment-members-query-create-segment-id-paths.json`,
      'config/parity-specs/segments/customer-segment-members-query-create-segment-id-paths.json',
      'config/parity-requests/segments/customer-segment-members-query-create-segment-id-paths.graphql',
      'config/parity-requests/segments/segment-create-for-member-query-segment-id-paths.graphql',
    ],
    cleanupBehavior:
      'Creates disposable segments for segmentId-backed success branches and deletes them during cleanup; malformed/wrong-resource GID cases need no store setup or cleanup; member-query jobs are async Shopify state without a cleanup mutation.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'segments',
    captureId: 'customer-segment-members-query-create-direct-query-grammar',
    scriptPath: 'scripts/capture-customer-segment-members-query-create-direct-query-grammar-conformance.ts',
    purpose:
      'customerSegmentMembersQueryCreate direct query branch accepted broad grammar and representative CDP malformed-query error shape.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'customer segment access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-segment-members-query-create-direct-query-grammar.json`,
      'config/parity-specs/segments/customer-segment-members-query-create-direct-query-grammar.json',
      'config/parity-requests/segments/customer-segment-members-query-create-direct-query-grammar.graphql',
    ],
    cleanupBehavior:
      'Creates customer segment member query jobs only; Shopify exposes them as async query state without a cleanup mutation.',
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
    captureId: 'online-store-page-blog-article-template-suffix',
    scriptPath: 'scripts/capture-online-store-template-suffix-conformance.ts',
    purpose:
      'Online store page/blog/article create and update templateSuffix persistence, empty-string preservation, explicit-null clearing, and downstream read-after-write behavior.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}page-blog-article-template-suffix.json`,
      'config/parity-specs/online-store/page-blog-article-template-suffix.json',
      'config/parity-requests/online-store/page-blog-article-template-suffix-article-create.graphql',
      'config/parity-requests/online-store/page-blog-article-template-suffix-blog-create.graphql',
      'config/parity-requests/online-store/page-blog-article-template-suffix-delete.graphql',
      'config/parity-requests/online-store/page-blog-article-template-suffix-page-create.graphql',
      'config/parity-requests/online-store/page-blog-article-template-suffix-read.graphql',
      'config/parity-requests/online-store/page-blog-article-template-suffix-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable blog, page, and article, updates their template suffixes, records empty-string/null handling and downstream reads, then deletes all created content during the scenario and retries cleanup if needed.',
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
    captureId: 'online-store-article-blog-not-found',
    scriptPath: 'scripts/capture-online-store-article-blog-not-found-conformance.ts',
    purpose:
      'articleCreate and articleUpdate blogId existence validation for references to non-existent blogs, including no-update readback.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}article-create-update-blog-not-found.json`,
      'config/parity-specs/online-store/article-create-update-blog-not-found.json',
      'config/parity-requests/online-store/article-create-update-blog-not-found-read.graphql',
      'config/parity-requests/online-store/online-store-article-create-validation-article-create.graphql',
      'config/parity-requests/online-store/online-store-article-update-validation-article-create.graphql',
      'config/parity-requests/online-store/online-store-article-update-validation-article-update.graphql',
      'config/parity-requests/online-store/online-store-article-update-validation-blog-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable blog and article setup record; unknown-blog create/update attempts should not mutate, and cleanup deletes the article then blog.',
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
    captureId: 'online-store-comment-moderation-status-enums-local-runtime',
    scriptPath: 'scripts/capture-online-store-comment-moderation-status-enums-local-runtime.ts',
    purpose:
      'Executable local-runtime evidence that commentSpam, commentNotSpam, and commentApprove persist Core comment status enum values from a cassette-hydrated comment.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}comment-moderation-status-enums.json`,
      'config/parity-specs/online-store/comment-moderation-status-enums.json',
      'config/parity-requests/online-store/comment-moderation-status-approve.graphql',
      'config/parity-requests/online-store/comment-moderation-status-not-spam.graphql',
      'config/parity-requests/online-store/comment-moderation-status-spam.graphql',
    ],
    cleanupBehavior:
      'Local-runtime only; the fixture replays a recorded comment hydrate response through the parity cassette and does not modify Shopify.',
    expectedStatusChecks: ['targeted-runtime-test', 'conformance:parity', 'conformance:check', 'rust:test'],
  },
  {
    domain: 'online-store',
    captureId: 'online-store-comment-moderation-not-found-codes',
    scriptPath: 'scripts/capture-online-store-comment-moderation-not-found-codes-conformance.ts',
    purpose:
      'commentApprove, commentSpam, commentNotSpam, and commentDelete unknown-id userErrors include NOT_FOUND codes.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}comment-moderation-not-found-codes.json`,
      'config/parity-specs/online-store/comment-moderation-not-found-codes.json',
      'config/parity-requests/online-store/comment-moderation-not-found-codes.graphql',
    ],
    cleanupBehavior:
      'Uses a non-existent comment GID to exercise resolver-level not-found branches without creating or mutating store records.',
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
      `${LOCAL_RUNTIME_ROOT}theme-update-validation-local-runtime.json`,
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
    captureId: 'online-store-mobile-platform-application-model-validation-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-online-store-mobile-platform-application-model-validation-local-runtime.ts',
    purpose:
      'mobilePlatformApplicationCreate model validation for application ID length, Android sha256CertFingerprints presence, and Apple appClipApplicationId presence/length.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      'config/parity-requests/online-store/mobile_platform_application_create_duplicate_android.graphql',
      `${LOCAL_RUNTIME_ROOT}mobile_platform_application_create_duplicate_platform.json`,
      `${LOCAL_RUNTIME_ROOT}mobile_platform_application_create_requires_one_platform.json`,
      'config/parity-specs/online-store/mobile_platform_application_create_duplicate_platform.json',
      `${LOCAL_RUNTIME_ROOT}mobile_platform_application_create_model_validation.json`,
      'config/parity-specs/online-store/mobile_platform_application_create_requires_one_platform.json',
      'config/parity-specs/online-store/mobile_platform_application_create_model_validation.json',
      'config/parity-requests/online-store/mobile_platform_application_create_model_validation.graphql',
    ],
    cleanupBehavior:
      'Local-runtime validation-only capture. Rejected mutations must return userErrors without staging records, so no Shopify or local cleanup is required.',
    expectedStatusChecks: ['targeted-runtime-test', 'conformance:parity', 'conformance:check', 'rust:test'],
    notes:
      'The current live conformance credential lacks mobile-platform read/write scopes; endpoint docs already record this scope blocker, so these Core-derived resolver branches are executable local-runtime evidence. Stale local-runtime parity specs for duplicate-platform and exact-platform create behavior are intentionally retired instead of being treated as Shopify evidence.',
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
    captureId: 'online-store-article-page-blog-length-validations',
    scriptPath: 'scripts/capture-online-store-length-validations-conformance.ts',
    purpose:
      'Online store article, blog, and page title/handle/body length validation branches plus public schema evidence for unsupported feedburner input fields.',
    requiredAuthScopes: ['read_content', 'write_content'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}article-page-blog-length-validations.json`,
      'config/parity-specs/online-store/article-page-blog-length-validations.json',
      'config/parity-requests/online-store/article-page-blog-length-validations-article-create.graphql',
      'config/parity-requests/online-store/article-page-blog-length-validations-article-update.graphql',
      'config/parity-requests/online-store/article-page-blog-length-validations-blog-create.graphql',
      'config/parity-requests/online-store/article-page-blog-length-validations-blog-update.graphql',
      'config/parity-requests/online-store/article-page-blog-length-validations-page-create.graphql',
      'config/parity-requests/online-store/article-page-blog-length-validations-page-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable blog, page, and article for update validation; invalid length attempts do not create records, and cleanup deletes the setup article, page, and blog.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Public Admin API 2025-01 rejects feedburner on BlogCreateInput and BlogUpdateInput before resolver execution; executable parity covers currently exposed title, handle, and body branches.',
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
    domain: 'online-store',
    captureId: 'online-store-integration-root-dispatch-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-online-store-integration-root-dispatch-local-runtime.ts',
    purpose:
      'Online store integration root dispatch: ensure themeCreate, themeUpdate, themeDelete, and themePublish route through the local dispatcher and return correct staged responses.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}online-store-integration-root-dispatch-local-runtime.json`,
      'config/parity-specs/online-store/online-store-integration-root-dispatch-local-runtime.json',
      'config/parity-requests/online-store/online-store-integration-root-dispatch-local-runtime.graphql',
      'config/parity-requests/online-store/online-store-integration-root-dispatch-delete-local-runtime.graphql',
      'config/parity-requests/online-store/online-store-integration-root-dispatch-read-local-runtime.graphql',
    ],
    cleanupBehavior:
      'Local-runtime only. Creates disposable themes through the proxy; cleanup is embedded in the capture script.',
    expectedStatusChecks: ['targeted-runtime-test', 'conformance:parity', 'conformance:check', 'rust:test'],
  },
  {
    domain: 'online-store',
    captureId: 'online-store-theme-publish-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-online-store-theme-publish-local-runtime.ts',
    purpose:
      'themePublish demotes the previous main theme and sets the published theme as new main; downstream reads reflect updated role.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}theme-publish-demotes-previous-main.json`,
      'config/parity-specs/online-store/theme-publish-demotes-previous-main.json',
      'config/parity-requests/online-store/theme-publish-create-main.graphql',
      'config/parity-requests/online-store/theme-publish-create-unpublished.graphql',
      'config/parity-requests/online-store/theme-publish-publish.graphql',
      'config/parity-requests/online-store/theme-publish-read.graphql',
    ],
    cleanupBehavior: 'Local-runtime only; all theme records are staged locally and no Shopify store state is modified.',
    expectedStatusChecks: ['targeted-runtime-test', 'conformance:parity', 'conformance:check', 'rust:test'],
  },
  {
    domain: 'online-store',
    captureId: 'online-store-theme-files-upsert-job-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-online-store-theme-files-upsert-job-local-runtime.ts',
    purpose: 'themeFilesUpsert returns job: null for inline writes and a synthetic Job payload for URL-body writes.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}theme-files-upsert-job.json`,
      'config/parity-specs/online-store/theme-files-upsert-job.json',
      'config/parity-requests/online-store/theme-files-upsert-job.graphql',
    ],
    cleanupBehavior:
      'Local-runtime only; theme and theme file records are staged locally and no Shopify store state is modified.',
    expectedStatusChecks: ['targeted-runtime-test', 'conformance:parity', 'conformance:check', 'rust:test'],
    notes:
      'The current proxy has no deterministic theme-limited-plan entitlement state, so this scenario covers the unconditionally emittable job payload branch while endpoint docs keep plan gating explicit as out of scope.',
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
      `${CAPTURE_ROOT}collections-catalog.json`,
      'config/parity-specs/products/collections-catalog-read.json',
      'config/parity-requests/products/collections-catalog-read.variables.json',
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
    captureId: 'collection-reorder-products-manual-sort',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-collection-reorder-products-conformance.mts',
    purpose:
      'collectionReorderProducts manual-sort success plus rejection for custom collections whose sortOrder is not MANUAL and smart collections that are not manually sorted.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}collection-reorder-products-manual-sort.json`,
      'config/parity-specs/products/collectionReorderProducts-parity-plan.json',
      'config/parity-specs/products/collectionReorderProducts-smart-collection.json',
      'config/parity-requests/products/collectionReorderProducts-parity-plan.graphql',
      'config/parity-requests/products/collectionReorderProducts-order-read.graphql',
      'config/parity-requests/products/collectionReorderProducts-collection-hydrate.graphql',
      'config/parity-requests/products/products-hydrate-nodes-observation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable manual, non-manual custom, and smart collections using existing products, captures success/rejection branches, then deletes all collections in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    captureId: 'collection-update-ruleset-job-parity',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-collection-update-ruleset-job-conformance.mts',
    purpose: 'collectionUpdate async job payload shape and ruleSet validation for custom collections and empty rules.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}collection-update-ruleset-job-parity.json`,
      'config/parity-specs/products/collection-update-ruleset-job-parity.json',
      'config/parity-requests/products/collectionUpdate-ruleset-job-create.graphql',
      'config/parity-requests/products/collectionUpdate-ruleset-job-update.graphql',
      'config/parity-requests/products/collectionUpdate-ruleset-job-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable custom and smart collections, captures update validation and job payloads, then deletes both collections.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'collections',
    captureId: 'collection-update-missing-id',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-collection-update-missing-id-conformance.mts',
    purpose: 'collectionUpdate missing input.id BadRequest shape and present-but-unknown id userError branch.',
    requiredAuthScopes: ['read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}collection-update-missing-id.json`,
      'config/parity-specs/products/collection-update-missing-id.json',
      'config/parity-requests/products/collectionUpdate-missing-id.graphql',
    ],
    cleanupBehavior: 'Validation-only probes do not create merchant resources.',
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
      'config/parity-specs/store-properties/shop-baseline-non-harry.json',
      'config/parity-requests/store-properties/shop-baseline-read.graphql',
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
      'config/parity-specs/store-properties/location-activate-generic-staging-readback.json',
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
      'locationAdd required-address/country-code validation, unsupported capabilities validation, address/default staging, and immediate read-after-write behavior.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-add-validation-and-defaults.json`,
      'config/parity-specs/store-properties/location-add-validation-and-defaults.json',
      'config/parity-specs/store-properties/location-add-generic-staging-readback.json',
      'config/parity-requests/store-properties/location-add-blank-name-code.graphql',
      'config/parity-requests/store-properties/location-add-capabilities-variable.graphql',
      'config/parity-requests/store-properties/location-add-inline-capabilities.graphql',
      'config/parity-requests/store-properties/location-add-inline-missing-country-code.graphql',
      'config/parity-requests/store-properties/location-add-invalid-country-code.graphql',
      'config/parity-requests/store-properties/location-add-missing-address.graphql',
      'config/parity-requests/store-properties/location-add-missing-country-code.graphql',
      'config/parity-requests/store-properties/location-add-read-after-add.graphql',
      'config/parity-requests/store-properties/location-add-validation-and-defaults.graphql',
    ],
    cleanupBehavior:
      'Creates disposable locations for default and explicit non-online fulfillment branches, then deactivates and deletes them.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    captureId: 'location-add-resource-limit-reached',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-location-add-resource-limit-reached-conformance.mts',
    purpose:
      'locationAdd public Admin API userError shape when the shop has reached its active merchant-managed location cap.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-add-resource-limit-reached.json`,
      'config/parity-specs/store-properties/location-add-resource-limit-reached.json',
      'config/parity-requests/store-properties/location-add-resource-limit-reached.graphql',
    ],
    cleanupBehavior:
      'Counts current active merchant-managed locations, creates disposable active locations until the shop reaches the captured locationLimit, records the first over-cap locationAdd response, then deactivates and deletes every disposable location created by the recorder.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    captureId: 'location-activate-limit-relocation-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-location-activate-limit-relocation-local-runtime.ts',
    purpose:
      'Local-runtime recording for locationActivate LOCATION_LIMIT, HAS_ONGOING_RELOCATION, and successful control branches; relocation message text is sourced from Shopify Core i18n because public Admin GraphQL cannot deterministically create an incomplete mass-relocation job.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}location-activate-limit-and-relocation.json`,
      'config/parity-specs/store-properties/location-activate-limit-and-relocation.json',
    ],
    cleanupBehavior:
      'Runs only against the local proxy runtime; the public disposable shop is not at its location limit and exposes no deterministic incomplete mass-relocation setup through Admin GraphQL, so no Shopify cleanup is required.',
    expectedStatusChecks: ['conformance:check', 'rust:test', 'targeted-runtime-test'],
  },
  {
    domain: 'store-properties',
    captureId: 'location-add-metafields',
    scriptPath: 'scripts/capture-location-add-metafields-conformance.mts',
    purpose:
      'locationAdd input.metafields staging, downstream location metafield reads, and metafield validation userErrors.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-add-metafields.json`,
      'config/parity-specs/store-properties/location-add-metafields.json',
      'config/parity-requests/store-properties/location-add-metafields.graphql',
      'config/parity-requests/store-properties/location-add-metafields-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable locations for the successful metafield branch and blank-value branch, then deactivates and deletes them.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'store-properties',
    captureId: 'location-address-code-derivation',
    scriptPath: 'scripts/capture-location-address-code-derivation-conformance.mts',
    purpose:
      'locationAdd/locationEdit country and province name derivation from supplied countryCode/provinceCode plus immediate read-after-write behavior.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-address-code-derivation.json`,
      'config/parity-specs/store-properties/location-address-code-derivation.json',
      'config/parity-requests/store-properties/location-address-code-derivation-add.graphql',
      'config/parity-requests/store-properties/location-address-code-derivation-edit.graphql',
      'config/parity-requests/store-properties/location-address-code-derivation-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable non-online-fulfilling GB, AU, and CA locations, edits the CA location provinceCode, reads each back, then deactivates and deletes them.',
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
    captureId: 'location-add-edit-uniqueness-and-required-fields',
    scriptPath: 'scripts/capture-location-add-edit-uniqueness-and-required-fields-conformance.mts',
    purpose:
      'locationAdd/locationEdit duplicate-name and length validation, plus observed locationAdd incomplete-address and US ZIP behavior.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-add-edit-uniqueness-and-required-fields.json`,
      'config/parity-specs/store-properties/location-add-edit-uniqueness-and-required-fields.json',
      'config/parity-requests/store-properties/location-add-edit-validation-add.graphql',
      'config/parity-requests/store-properties/location-add-edit-validation-edit.graphql',
    ],
    cleanupBehavior:
      'Creates disposable locations for setup and for observed successful incomplete-address/ZIP branches, then deactivates and deletes them.',
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
    captureId: 'location-activate-fulfillment-service-scope',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-location-activate-fulfillment-service-scope-conformance.mts',
    purpose:
      'locationActivate fulfillment-service-managed Location scope behavior plus downstream read-after-write state.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-activate-fulfillment-service-scope.json`,
      'config/parity-specs/store-properties/location-activate-fulfillment-service-scope.json',
      'config/parity-requests/store-properties/location-activate-fulfillment-service-scope.graphql',
      'config/parity-requests/store-properties/location-activate-fulfillment-service-scope-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable fulfillment service, records activation scope rejection and read-back for the associated Location, then deletes the fulfillment service.',
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
      'config/parity-specs/store-properties/location-delete-primary-location.json',
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
    captureId: 'location-delete-inventory-level-cascade',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-location-delete-inventory-level-cascade-conformance.mts',
    purpose:
      'locationDelete downstream inventoryItem inventoryLevels and locationsCount cascade parity after deleting a stocked disposable location.',
    requiredAuthScopes: ['read_locations', 'write_locations', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}location-delete-inventory-level-cascade.json`,
      'config/parity-specs/store-properties/location-delete-inventory-level-cascade.json',
      'config/parity-requests/store-properties/location-delete-inventory-product-create.graphql',
      'config/parity-requests/store-properties/location-delete-inventory-product-track.graphql',
      'config/parity-requests/store-properties/location-delete-inventory-location-add.graphql',
      'config/parity-requests/store-properties/location-delete-inventory-activate.graphql',
      'config/parity-requests/store-properties/location-delete-inventory-deactivate.graphql',
      'config/parity-requests/store-properties/location-delete-inventory-delete.graphql',
      'config/parity-requests/store-properties/location-delete-inventory-read.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable merchant-managed locations and one disposable tracked product, stocks the product at both locations, deactivates/deletes the target location, deletes the product, and removes any remaining disposable locations.',
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
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/store-properties/shop-policy-update-user-error-codes.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/store-properties/shop-policy-update-parity.json',
      'config/parity-specs/store-properties/shopPolicyUpdate-parity.json',
      'config/parity-specs/store-properties/shop-policy-update-title-url-and-body-rendering.json',
      'config/parity-specs/store-properties/shop-policy-update-user-error-codes.json',
    ],
    cleanupBehavior:
      'Restores prior policy content when a write branch is captured. Newly created policy rows may remain on shops where Shopify does not expose deletion, but their bodies are reset to the prior empty fallback.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'store-properties',
    captureId: 'shop-policy-privacy-liquid-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-shop-policy-privacy-liquid-validation-conformance.ts',
    purpose: 'shopPolicyUpdate PRIVACY_POLICY Liquid syntax validation for invalid policy bodies.',
    requiredAuthScopes: ['read_content', 'write_content or policy-management access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}shop-policy-update-privacy-liquid-validation.json`,
      'config/parity-specs/store-properties/shop-policy-update-privacy-liquid-validation.json',
      'config/parity-requests/store-properties/shopPolicyUpdate-user-error-codes.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture. Rejected privacy-policy Liquid syntax writes must return a body userError and must not mutate policy content.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
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
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/privacy/data-sale-opt-out-missing-email.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/privacy/data-sale-opt-out-invalid-format.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/privacy/data-sale-opt-out-new-customer-defaults.json',
      'config/parity-specs/privacy/data-sale-opt-out-invalid-format.json',
      'config/parity-requests/privacy/data-sale-opt-out-customer-lookup.graphql',
    ],
    cleanupBehavior:
      'Creates/deletes disposable customer records for opt-out probes; invalid-format capture requires no setup and deletes any unexpectedly created customer before failing.',
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
    domain: 'admin-platform',
    captureId: 'graphql-base-validation-unhappy-paths',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-graphql-base-validation-conformance.ts',
    purpose:
      'Base Admin GraphQL validation unhappy paths for parse errors, missing subselections, omitted variables, missing required root arguments, unknown schema fields, and required input-object properties.',
    requiredAuthScopes: [
      'active Admin API token with Admin GraphQL schema access',
      'read_products',
      'write_products',
      'write_webhooks',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}graphql-base-validation-unhappy-paths.json`,
      'config/parity-specs/admin-platform/graphql-base-validation-unhappy-paths.json',
      'config/parity-requests/admin-platform/graphql-base-validation-invalid-syntax.graphql',
      'config/parity-requests/admin-platform/graphql-base-validation-missing-input-required-property.graphql',
      'config/parity-requests/admin-platform/graphql-base-validation-missing-required-argument.graphql',
      'config/parity-requests/admin-platform/graphql-base-validation-missing-required-variable.graphql',
      'config/parity-requests/admin-platform/graphql-base-validation-missing-subselection.graphql',
      'config/parity-requests/admin-platform/graphql-base-validation-unknown-mutation-root.graphql',
      'config/parity-requests/admin-platform/graphql-base-validation-unknown-product-field.graphql',
      'config/parity-requests/admin-platform/graphql-base-validation-unknown-query-root.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture; requests fail GraphQL parsing, validation, or variable coercion before resolver execution and do not mutate store data.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
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
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/orders/order-merchant-detail-read.json',
      'config/parity-specs/orders/fulfillment-lifecycle-create-update-cancel.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-orders-catalog.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-orders-count.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2025-01/orders/draft-orders-invalid-email-query.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/fulfillment-cancel-parity.json',
      'config/parity-specs/orders/fulfillment-lifecycle-create-update-cancel.json',
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
    captureId: 'draft-orders-status-query-read',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-draft-orders-status-query-conformance.ts',
    purpose:
      'Live draftOrders/draftOrdersCount read for a valid status:open draft-order search query, recorded as an upstream cassette for cold proxy replay.',
    requiredAuthScopes: ['read_draft_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draft-orders-status-query-read.json`,
      'config/parity-specs/orders/draftOrders-status-query-read.json',
      'config/parity-requests/orders/draftOrders-status-query-read.graphql',
      'config/parity-requests/orders/draftOrders-status-query-read.variables.json',
    ],
    cleanupBehavior: 'Read-only capture; does not create, update, or delete Shopify resources.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-catalog-count-read',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-catalog-count-read-conformance.mts',
    purpose:
      'Re-homed (from very-big-test-store) orders/ordersCount catalog read: forwards the multi-alias catalog query and the cursor-threaded next-page query against a live merchant-realistic order catalog on harry-test-heelo, recording both as cassettes so the proxy answers by forward-and-observe instead of a seeded catalog.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/orders/order-catalog-count-read.json',
      'config/parity-specs/orders/order-catalog-count-read.json',
    ],
    cleanupBehavior:
      'Creates disposable paid test orders tagged merchant-realistic only if fewer than two exist; leaves them in place as the durable read catalog.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
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
    captureId: 'order-delete-snapshot-staging',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-delete-snapshot-staging-conformance.ts',
    purpose:
      'orderDelete success, read-after-delete cascade, repeat-delete NOT_FOUND, paid fulfilled-order delete success, and unknown-order NOT_FOUND behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}order-delete-snapshot-staging.json`,
      'config/parity-specs/orders/orderDelete-snapshot-staging.json',
      'config/parity-requests/orders/orderDelete-snapshot-staging-create.graphql',
      'config/parity-requests/orders/orderDelete-snapshot-staging-delete.graphql',
      'config/parity-requests/orders/orderDelete-snapshot-staging-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable deletable and fulfilled orders; successful delete removes the deletable order and cleanup cancels/deletes any remaining fulfilled order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-customer-error-paths',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-customer-error-paths-conformance.ts',
    purpose:
      'orderCustomerSet B2B no-role translated userError, orderCustomerRemove B2B rejection, and cancelled non-B2B removal success.',
    requiredAuthScopes: [
      'read_orders',
      'write_orders',
      'read_customers',
      'write_customers',
      'read_companies',
      'write_companies',
      'write_draft_orders',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderCustomerSet-and-Remove-error-paths.json`,
      `${CAPTURE_ROOT}orderCustomerSet-and-Remove-error-paths-cleanup.json`,
      'config/parity-specs/orders/orderCustomerSet-and-Remove-error-paths.json',
      'config/parity-requests/orders/orderCustomerRemove-error-paths.graphql',
      'config/parity-requests/orders/orderCustomerSet-error-paths.graphql',
      'config/parity-requests/orders/orderCustomer-error-paths-customer-create.graphql',
      'config/parity-requests/orders/orderCancel-state-transitions-order-create.graphql',
      'config/parity-requests/orders/orderCancel-parity.graphql',
      'config/parity-requests/b2b/b2b-contact-business-rules-company-create.graphql',
      'config/parity-requests/b2b/b2b-contact-business-rules-draft-order-create.graphql',
      'config/parity-requests/b2b/b2b-contact-business-rules-draft-order-complete.graphql',
      'config/parity-requests/b2b/b2b-company-contact-main-delete-assign-customer.graphql',
    ],
    cleanupBehavior:
      'Creates disposable customers, company/contact/location setup, one B2B order, and one cancelled non-B2B order; cleanup cancels orders and best-effort deletes customers/company.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-cancel-error-messages',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-cancel-error-messages-conformance.ts',
    purpose:
      'orderCancel validation userError wording for staffNote length, refund/refundMethod conflicts with refund true and false, and already-cancelled orders.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderCancel-error-messages.json`,
      'config/parity-specs/orders/orderCancel-error-messages.json',
      `${LOCAL_RUNTIME_ROOT}orderCancel-state-transitions.json`,
      'config/parity-specs/orders/orderCancel-state-transitions.json',
      'config/parity-requests/orders/orderCancel-error-messages-order-create.graphql',
      'config/parity-requests/orders/orderCancel-error-messages.graphql',
      'config/parity-requests/orders/orderCancel-error-messages-setup-cancel.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable test orders; one is cancelled to capture already-cancelled behavior and the other is cancelled during cleanup after validation-only branches.',
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
      `${CAPTURE_ROOT}orderCancel-restock-refundMethod-parity.json`,
      `${CAPTURE_ROOT}orderClose-parity.json`,
      `${CAPTURE_ROOT}orderCreateManualPayment-access-denied-parity.json`,
      `${CAPTURE_ROOT}orderCustomerRemove-parity.json`,
      `${CAPTURE_ROOT}orderCustomerSet-parity.json`,
      `${CAPTURE_ROOT}orderInvoiceSend-parity.json`,
      `${CAPTURE_ROOT}orderOpen-parity.json`,
      `${CAPTURE_ROOT}taxSummaryCreate-access-denied-parity.json`,
      `${CAPTURE_ROOT}order-management-cleanup.json`,
      `${LOCAL_RUNTIME_ROOT}orderCustomerSet-and-Remove-error-paths.json`,
      'config/parity-specs/orders/orderCancel-parity.json',
      'config/parity-specs/orders/orderCancel-restock-refundMethod-parity.json',
      'config/parity-specs/orders/orderCancel-snapshot-staging.json',
      'config/parity-specs/orders/orderClose-parity.json',
      'config/parity-specs/orders/orderClose-snapshot-staging.json',
      'config/parity-specs/orders/orderCreateManualPayment-access-denied-parity.json',
      'config/parity-specs/orders/orderCustomerRemove-parity.json',
      'config/parity-specs/orders/orderCustomerSet-parity.json',
      'config/parity-specs/orders/orderCustomerSet-and-Remove-error-paths.json',
      'config/parity-specs/orders/orderInvoiceSend-parity.json',
      'config/parity-specs/orders/orderOpen-parity.json',
      'config/parity-specs/orders/orderOpen-snapshot-staging.json',
      'config/parity-specs/orders/taxSummaryCreate-access-denied-parity.json',
      'config/parity-requests/orders/orderCancel-parity.graphql',
      'config/parity-requests/orders/orderCancel-restock-refundMethod-downstream-read.graphql',
      'config/parity-requests/orders/orderCancel-restock-refundMethod-parity.graphql',
      'config/parity-requests/orders/orderClose-parity.graphql',
      'config/parity-requests/orders/orderClose-snapshot-staging-setup.graphql',
      'config/parity-requests/orders/orderCreateManualPayment-access-denied-parity.graphql',
      'config/parity-requests/orders/orderCustomerRemove-parity.graphql',
      'config/parity-requests/orders/orderCustomerRemove-error-paths.graphql',
      'config/parity-requests/orders/orderCustomerSet-parity.graphql',
      'config/parity-requests/orders/orderCustomerSet-error-paths.graphql',
      'config/parity-requests/orders/orderInvoiceSend-parity.graphql',
      'config/parity-requests/orders/orderOpen-parity.graphql',
      'config/parity-requests/orders/orderOpen-snapshot-staging-setup.graphql',
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
    domain: 'draft-orders',
    captureId: 'draft-order-complete-parity',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-draft-order-complete-parity-conformance.ts',
    purpose:
      'De-seeded draftOrderComplete live parity (re-homed from very-big-test-store to harry-test-heelo): completes a disposable fully-ready draft and records the single cold OrdersDraftOrderHydrate forward the proxy uses to resolve the precondition draft instead of a setup-block seed.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draft-order-complete-parity.json`,
      'config/parity-specs/orders/draftOrderComplete-parity-plan.json',
      'config/parity-requests/orders/draft-order-hydrate.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable draft with non-taxable custom line items, completes it, then cancels the resulting order (restock:false) in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-complete-payment-terms-pending',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-draft-order-complete-payment-terms-pending-conformance.ts',
    purpose:
      'Completes a disposable payment-terms draft without paymentPending and records Shopify leaving the resulting order pending with no captured payment.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draft-order-complete-payment-terms-pending.json`,
      'config/parity-specs/orders/draftOrderComplete-payment-terms-pending.json',
      'config/parity-requests/orders/draftOrderComplete-payment-terms-pending.graphql',
      'config/parity-requests/orders/draftOrderComplete-payment-terms-pending-order-read.graphql',
      'config/parity-requests/orders/draft-order-hydrate.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable draft with non-taxable custom line items, attaches payment terms, completes it, then cancels the resulting order (restock:false) in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-delete-parity',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-draft-order-delete-conformance.ts',
    purpose:
      'De-seeded draftOrderDelete live parity: deletes a disposable draft and records the cold OrdersDraftOrderHydrate forward that resolves the precondition draft instead of a setup-block seed.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/draft-order-delete-parity.json',
    ],
    cleanupBehavior:
      'Creates one disposable draft and deletes it as the scenario operation; no residual records remain.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-duplicate-parity',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-draft-order-duplicate-conformance.ts',
    purpose:
      'De-seeded draftOrderDuplicate live parity: duplicates a disposable draft and records the cold OrdersDraftOrderHydrate forward that resolves the precondition draft instead of a setup-block seed.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/draft-order-duplicate-parity.json',
    ],
    cleanupBehavior:
      'Creates one disposable draft, duplicates it, then deletes both the source and duplicate drafts in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-update-parity',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-draft-order-update-conformance.ts',
    purpose:
      'De-seeded draftOrderUpdate live parity: updates a disposable draft and records the cold OrdersDraftOrderHydrate forward that resolves the precondition draft instead of a setup-block seed.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/draft-order-update-parity.json',
    ],
    cleanupBehavior: 'Creates one disposable draft, applies the update, then deletes it in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-edit-existing-order',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-edit-existing-order-conformance.ts',
    purpose:
      'De-seeded order-edit existing-order happy-path and validation live parity (re-homed from very-big-test-store to harry-test-heelo): begins an edit on a disposable order and records the cold OrdersOrderEditHydrate (and variant hydrate) forwards the proxy uses instead of a setup-block seed.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}order-edit-existing-order-happy-path.json`,
      `${CAPTURE_ROOT}order-edit-existing-order-validation.json`,
      'config/parity-specs/orders/orderEditExistingOrder-happy-path.json',
      'config/parity-specs/orders/orderEditExistingOrder-validation.json',
      'config/parity-specs/orders/orderEditBegin-parity-plan.json',
      'config/parity-specs/orders/orderEditAddVariant-parity-plan.json',
      'config/parity-specs/orders/orderEditCommit-parity-plan.json',
      'config/parity-requests/orders/order-edit-hydrate.graphql',
      'config/parity-requests/orders/orderEditExistingWorkflow-begin.graphql',
      'config/parity-requests/orders/orderEditExistingWorkflow-addVariant.graphql',
      'config/parity-requests/orders/orderEditExistingWorkflow-addVariant-payload.graphql',
    ],
    cleanupBehavior:
      'Creates disposable orders with custom line items, runs order-edit sessions, then cancels the orders in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-edit-residual-calculated-edits',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-order-edit-residual-conformance.ts',
    purpose:
      'De-seeded order-edit residual calculated-edits live parity: runs begin/addCustomItem/discount/shipping-line edits on a disposable order and records the cold OrdersOrderEditHydrate forward instead of a setup-block seed.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_products', 'write_products'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/order-edit-residual-calculated-edits.json',
    ],
    cleanupBehavior:
      'Creates one disposable order with a non-taxable custom line, runs the residual edit workflow, then cancels the order in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-edit-zero-removal',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-edit-zero-removal-conformance.ts',
    purpose:
      'De-seeded order-edit zero-removal live parity (re-homed from very-big-test-store to harry-test-heelo): zeroes a line on a disposable order edit, commits, and records the cold OrdersOrderEditHydrate forward instead of a setup-block seed.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}order-edit-existing-order-zero-removal.json`,
      'config/parity-specs/orders/orderEditExistingOrder-zero-removal.json',
      'config/parity-specs/orders/orderEditSetQuantity-parity-plan.json',
      'config/parity-requests/orders/orderEditExistingWorkflow-setQuantity.graphql',
      'config/parity-requests/orders/orderEditExistingWorkflow-setQuantity-payload.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable order with custom line items, zeroes a line and commits the edit, then cancels the order in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-edit-commit-success-messages',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-edit-commit-success-messages-conformance.ts',
    purpose:
      'orderEditCommit successMessages for notifyCustomer false, paid notify, balance-due notify, and closed-order unarchive commit branches.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}order-edit-commit-success-messages.json`,
      'config/parity-specs/orders/orderEditCommit-success-messages.json',
      'config/parity-requests/orders/orderEditCommit-success-messages-create.graphql',
      'config/parity-requests/orders/orderEditCommit-success-messages-begin.graphql',
      'config/parity-requests/orders/orderEditCommit-success-messages-setQuantity.graphql',
      'config/parity-requests/orders/orderEditCommit-success-messages-close.graphql',
      'config/parity-requests/orders/orderEditCommit-success-messages-commit.graphql',
    ],
    cleanupBehavior:
      'Creates disposable paid, balance-due, and closed orders, commits order edits including notifyCustomer payload branches, then cancels created orders in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-lifecycle-noop-rehome',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-order-lifecycle-noop-conformance.ts',
    purpose:
      'De-seeded orderClose/orderOpen no-op live parity (re-homed to harry-test-heelo): redundant close/open on disposable orders preserve timestamps and return silent-success payloads, recording the cold order hydrate forward instead of a setup-block seed.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/orderClose-noop-on-already-closed.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/orders/orderOpen-noop-on-already-open.json',
    ],
    cleanupBehavior:
      'Creates disposable orders, reopens the closed-order probe after capture, and cancels both orders in best-effort cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-create-math-matrix',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-order-create-math-matrix-conformance.ts',
    purpose:
      'orderCreate subtotal, shipping, tax, discount, payment state, capturable, and presentment MoneyBag math against disposable orders.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}order-create-math-matrix.json`,
      'config/parity-specs/orders/orderCreate-math-matrix.json',
      'config/parity-requests/orders/orderCreate-math-matrix.graphql',
    ],
    cleanupBehavior:
      'Creates disposable test orders for each math branch, then records best-effort orderCancel cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-create-line-item-fields',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-order-create-line-item-fields-conformance.ts',
    purpose:
      'orderCreate line-item properties/customAttributes, shipping/taxable flags, vendor, product linkage, priceSet, empty discount allocations, and downstream read-after-write behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}order-create-line-item-fields.json`,
      'config/parity-specs/orders/orderCreate-line-item-fields.json',
      'config/parity-requests/orders/orderCreate-line-item-fields.graphql',
      'config/parity-requests/orders/orderCreate-line-item-fields-downstream-read.graphql',
    ],
    cleanupBehavior: 'Creates one disposable test order, reads it back, then records best-effort orderCancel cleanup.',
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
    captureId: 'order-update-localization-and-staff',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-update-localization-and-staff-conformance.ts',
    purpose:
      'orderUpdate localizedFields/localizationExtensions read-after-write parity plus public-schema evidence that staffMemberId is unavailable on the configured conformance store.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderUpdate-localization-and-staff.json`,
      'config/parity-specs/orders/orderUpdate-localization-and-staff.json',
      'config/parity-requests/orders/orderUpdate-localization-and-staff.graphql',
      'config/parity-requests/orders/orderUpdate-localization-and-staff-read.graphql',
      'config/parity-requests/orders/orderUpdate-localization-and-staff-unknown-staff.graphql',
    ],
    cleanupBehavior: 'Creates a disposable paid test order and cancels it after capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The staffMemberId NOT_FOUND branch is runtime-test-backed because the public 2026-04 schema for the configured conformance store rejects OrderInput.staffMemberId before resolver execution.',
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
    captureId: 'fulfillment-multi-tracking-info',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-fulfillment-multi-tracking-conformance.ts',
    purpose:
      'fulfillmentCreate and fulfillmentTrackingInfoUpdate multi-package tracking numbers/urls behavior and downstream order fulfillment visibility.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-multi-tracking-info.json`,
      'config/parity-specs/orders/fulfillment-multi-tracking-info.json',
      'config/parity-requests/orders/fulfillmentCreate-multi-tracking.graphql',
      'config/parity-requests/orders/fulfillmentTrackingInfoUpdate-multi-tracking.graphql',
      'config/parity-requests/orders/fulfillment-multi-tracking-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable paid test order, captures fulfillment create and tracking update behavior, then cancels/deletes the order where Shopify permits cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The public Admin schema for the configured API exposes FulfillmentTrackingInput numbers/urls fields; trackingDetails/trackingCompany are not accepted by Shopify for this root.',
  },
  {
    domain: 'orders',
    captureId: 'fulfillment-state-preconditions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-state-preconditions-conformance.ts',
    purpose:
      'fulfillmentCancel, fulfillmentTrackingInfoUpdate, and fulfillmentEventCreate behavior for cancelled and delivered fulfillment states.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-state-preconditions.json`,
      'config/parity-specs/orders/fulfillment-state-preconditions.json',
    ],
    cleanupBehavior:
      'Creates disposable paid test orders, records cancelled and delivered fulfillment branches, then cancels orders where Shopify permits cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Public Admin GraphQL 2026-04 treats already-cancelled cancel, tracking update on cancelled fulfillment, event creation on cancelled fulfillment, and cancel after delivered event as accepted branches with empty userErrors.',
  },
  {
    domain: 'orders',
    captureId: 'order-edit-lifecycle-user-errors',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-edit-lifecycle-user-errors-conformance.mts',
    purpose:
      'orderEditBegin/AddVariant/SetQuantity/AddLineItemDiscount/Commit missing-resource and rendered userError message payloads for lifecycle validation.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}order-edit-lifecycle-user-errors.json`,
      'config/parity-specs/orders/orderEdit-lifecycle-userErrors.json',
      'config/parity-requests/orders/orderEdit-lifecycle-userErrors-addVariant.graphql',
      'config/parity-requests/orders/orderEdit-lifecycle-userErrors-begin.graphql',
      'config/parity-requests/orders/orderEdit-lifecycle-userErrors-commit.graphql',
      'config/parity-requests/orders/orderEdit-lifecycle-userErrors-setQuantity.graphql',
    ],
    cleanupBehavior:
      'Creates disposable test orders for open-session and not-editable order-edit branches, then cancels them after recording; missing calculated-order probes use absent Shopify GIDs.',
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
    domain: 'orders',
    captureId: 'order-edit-shipping-line-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-edit-shipping-line-validation-conformance.ts',
    purpose:
      'orderEditAddShippingLine, orderEditUpdateShippingLine, orderEditRemoveShippingLine, orderEditRemoveDiscount, and orderEditAddLineItemDiscount validation branches against a disposable order-edit session.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderEdit-shipping-line-validation.json`,
      'config/parity-specs/orders/orderEdit-shipping-line-validation.json',
      'config/parity-requests/orders/orderEdit-shipping-line-validation-add-line-item-discount.graphql',
      'config/parity-requests/orders/orderEdit-shipping-line-validation-add-missing-price.graphql',
      'config/parity-requests/orders/orderEdit-shipping-line-validation-add.graphql',
      'config/parity-requests/orders/orderEdit-shipping-line-validation-begin.graphql',
      'config/parity-requests/orders/orderEdit-shipping-line-validation-discount-missing-currency.graphql',
      'config/parity-requests/orders/orderEdit-shipping-line-validation-remove.graphql',
      'config/parity-requests/orders/orderEdit-shipping-line-validation-remove-discount.graphql',
      'config/parity-requests/orders/orderEdit-shipping-line-validation-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable CAD test order, begins an order edit, records rejected shipping-line and discount validation branches, then cancels the order with restock.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-edit-quantity-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-edit-quantity-validation-conformance.ts',
    purpose:
      'orderEditSetQuantity negative quantity and orderEditAddVariant zero/negative quantity validation against a disposable order-edit session.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderEdit-quantity-validation.json`,
      'config/parity-specs/orders/orderEdit-quantity-validation.json',
      'config/parity-requests/orders/orderEdit-quantity-validation-addVariant.graphql',
      'config/parity-requests/orders/orderEdit-quantity-validation-begin.graphql',
      'config/parity-requests/orders/orderEdit-quantity-validation-setQuantity.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable CAD test order, begins an order edit, records rejected quantity branches and a read-after-reject addVariant baseline, then cancels the order with restock.',
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
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-create-name',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-fulfillment-create-name-conformance.ts',
    purpose:
      'fulfillmentCreate Fulfillment.name reference-number payloads for two fulfillments on the same disposable order.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-create-name.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-create-name.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-create-name.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable paid test order with quantity two, records two fulfillmentCreate payloads against the same fulfillment order, then cancels/deletes the order where Shopify permits cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-family',
    scriptPath: 'scripts/capture-draft-order-family-conformance.mts',
    purpose: 'Draft order create/update/delete/complete, duplicate lifecycle reset, and downstream read behavior.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders', 'read_products'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}draft-order-complete-payment-gateway-paths.json`,
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
      'config/parity-specs/orders/draftOrderComplete-non-recording-operation-name.json',
      'config/parity-requests/orders/draftOrderComplete-non-recording-operation-create.graphql',
      'config/parity-requests/orders/draftOrderComplete-non-recording-operation-complete.graphql',
      'config/parity-requests/orders/draftOrderComplete-non-recording-operation-read-by-id.graphql',
      'config/parity-requests/orders/draftOrderComplete-non-recording-operation-read-by-name.graphql',
    ],
    cleanupBehavior: 'Creates disposable draft orders and deletes/completes/cancels them per branch.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-complete-already-paid',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-draft-order-complete-already-paid-conformance.ts',
    purpose:
      'draftOrderComplete state-machine guard for rejecting a second completion after the draft has already been paid.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders', 'read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draft-order-complete-already-paid.json`,
      'config/parity-specs/orders/draftOrderComplete-already-paid.json',
      'config/parity-requests/orders/draftOrderComplete-already-paid-create.graphql',
      'config/parity-requests/orders/draftOrderComplete-already-paid-complete.graphql',
      'config/parity-requests/orders/draftOrderComplete-already-paid-order-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable draft order, completes it, records the rejected second completion and before/after reads of the resulting order, then attempts to cancel the order.',
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
    captureId: 'draft-order-bulk-tag-case-preservation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-draft-order-bulk-tag-case-preservation-conformance.ts',
    purpose:
      'draftOrderBulkAddTags trim, case-insensitive dedupe, and original display-case preservation read-after-write behavior.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draft-order-bulk-tag-case-preservation.json`,
      'config/parity-specs/orders/draftOrderBulkTag-case-preservation.json',
    ],
    cleanupBehavior: 'Creates one disposable draft order, records bulk tag add readback, then deletes it.',
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
    captureId: 'draft-order-variant-custom-only-fields',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-draft-order-variant-custom-only-fields-conformance.ts',
    purpose:
      'draftOrderCreate and draftOrderCalculate line items with variantId ignore custom-only title, sku, price, taxable, and requiresShipping input fields and return hydrated catalog values.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draft-order-variant-custom-only-fields.json`,
      'config/parity-specs/orders/draftOrder-variant-custom-only-fields.json',
      'config/parity-requests/orders/draft-order-variant-custom-only-create.graphql',
      'config/parity-requests/orders/draft-order-variant-custom-only-calculate.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable draft product/variant and one draft order, captures create/calculate normalization, then deletes the draft and product in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-line-items-max',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-draft-order-line-items-max-conformance.ts',
    purpose:
      'DraftOrderInput.lineItems max-input-size top-level GraphQL errors for draftOrderCreate, draftOrderUpdate, and draftOrderCalculate.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draftOrder-line-items-max.json`,
      'config/parity-specs/orders/draftOrder-line-items-max.json',
      'config/parity-requests/orders/draftOrder-line-items-max.graphql',
    ],
    cleanupBehavior: 'Validation-only max-input probes do not create merchant resources.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-tag-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-draft-order-tag-validation-conformance.ts',
    purpose:
      'DraftOrderInput tag count and per-tag length validation for draftOrderCreate, draftOrderUpdate, and draftOrderCalculate.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draftOrder-tag-validation.json`,
      'config/parity-specs/orders/draftOrder-tag-validation.json',
      'config/parity-requests/orders/draftOrder-tag-validation-create.graphql',
      'config/parity-requests/orders/draftOrder-tag-validation-update.graphql',
      'config/parity-requests/orders/draftOrder-tag-validation-calculate.graphql',
      'config/parity-requests/orders/draftOrder-tag-validation-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable draft orders for setup and normalized-count acceptance, captures rejected validation branches, and deletes created draft orders after capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-applied-discount-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-draft-order-applied-discount-validation-conformance.ts',
    purpose:
      'DraftOrderAppliedDiscountInput percentage value bounds, decimal precision, and valueType coercion for draftOrderCreate, draftOrderUpdate, and draftOrderCalculate.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draftOrder-applied-discount-validation.json`,
      'config/parity-specs/orders/draftOrder-applied-discount-validation.json',
      'config/parity-requests/orders/draftOrder-applied-discount-validation-setup.graphql',
      'config/parity-requests/orders/draftOrder-applied-discount-validation-create.graphql',
      'config/parity-requests/orders/draftOrder-applied-discount-validation-update.graphql',
      'config/parity-requests/orders/draftOrder-applied-discount-validation-calculate.graphql',
      'config/parity-requests/orders/draftOrder-applied-discount-value-type-required.graphql',
    ],
    cleanupBehavior:
      'Creates one setup draft order plus accepted validation branches, captures rejected validation/coercion branches, and deletes created draft orders after capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Live 2025-01 and 2026-04 probes accepted negative percentage discounts, so the parity-backed local validation intentionally does not reject them.',
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-invoice-send-safety',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-draft-order-invoice-send-safety-conformance.ts',
    purpose: 'Safety probes for draftOrderInvoiceSend side effects and validation branches.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draft-order-invoice-send-safety.json`,
      'config/parity-specs/orders/draftOrderInvoiceSend-parity-plan.json',
    ],
    cleanupBehavior: 'Uses safety-first validation branches; review manually before any customer-visible send path.',
    expectedStatusChecks: [...DEFAULT_STATUS_CHECKS, 'manual-capture-review'],
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-invoice-send-status-transition',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-draft-order-invoice-send-status-transition-conformance.ts',
    purpose:
      'Successful draftOrderInvoiceSend status and invoiceSentAt transition plus immediate draftOrder read-after-write.',
    requiredAuthScopes: ['read_draft_orders', 'write_draft_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}draft-order-invoice-send-status-transition.json`,
      'config/parity-specs/orders/draftOrderInvoiceSend-status-transition.json',
      'config/parity-requests/orders/draftOrderInvoiceSend-status-transition-create.graphql',
      'config/parity-requests/orders/draftOrderInvoiceSend-status-transition-send.graphql',
      'config/parity-requests/orders/draftOrderInvoiceSend-status-transition-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable draft order with a reserved example.com recipient, records successful invoice send and read-back, then deletes the draft order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'draft-orders',
    captureId: 'draft-order-invoice-send-invoice-errors-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-draft-order-invoice-send-invoice-errors-local-runtime.ts',
    purpose:
      'Local-runtime recording for private draftOrderInvoiceSend invoiceErrors plus presentment/template invoice metadata.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}draft-order-invoice-send-invoice-errors.json`,
      'config/parity-specs/orders/draftOrderInvoiceSend-invoice-errors.json',
      'config/parity-requests/orders/draftOrderInvoiceSend-invoice-errors-create.graphql',
      'config/parity-requests/orders/draftOrderInvoiceSend-invoice-errors-send.graphql',
    ],
    cleanupBehavior:
      'Runs only against the local proxy runtime because the public Admin schema does not expose the private invoice error field or template/presentment arguments; no Shopify cleanup required.',
    expectedStatusChecks: ['conformance:check', 'rust:test', 'targeted-runtime-test'],
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
      'config/parity-specs/discounts/discount-redeem-code-bulk-delete-validation.json',
      'config/parity-requests/discounts/discount-redeem-code-bulk-delete-setup.graphql',
      'config/parity-requests/discounts/discount-redeem-code-bulk-delete-validation.graphql',
      'config/parity-requests/discounts/discount-redeem-code-bulk-delete-happy.graphql',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-status-time-window-derivation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-timestamps-monotonic.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-update-edge-cases.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-validation-branches.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-value-bounds.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-automatic-basic-detail-read.json',
      'config/parity-specs/discounts/discount-automatic-basic-detail-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-catalog-code-filter-empty-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-catalog-empty-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-catalog-non-empty-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-catalog-status-filter-read.json',
      'fixtures/conformance/very-big-test-store.myshopify.com/2026-04/discounts/discount-code-basic-detail-read.json',
      'config/parity-specs/discounts/discount-code-basic-detail-read.json',
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
    captureId: 'discount-add-remove-overlap',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-add-remove-overlap-conformance.ts',
    purpose: 'Discount customer, item, and country add/remove overlap top-level BAD_REQUEST validation branches.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_products'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/discounts/discount-add-remove-overlap.json',
      'config/parity-specs/discounts/discount-add-remove-overlap.json',
      'config/parity-requests/discounts/discount-add-remove-overlap-setup.graphql',
      'config/parity-requests/discounts/discount-add-remove-overlap-basic-create.graphql',
      'config/parity-requests/discounts/discount-add-remove-overlap-basic-update.graphql',
      'config/parity-requests/discounts/discount-add-remove-overlap-bxgy-create.graphql',
      'config/parity-requests/discounts/discount-add-remove-overlap-bxgy-update.graphql',
      'config/parity-requests/discounts/discount-add-remove-overlap-free-shipping-create.graphql',
      'config/parity-requests/discounts/discount-add-remove-overlap-free-shipping-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable setup discounts for update probes, records validation-only overlap failures, then deletes the setup discounts.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-code-basic-lifecycle-conformance.ts',
    purpose: 'Code discount basic create/update/delete lifecycle.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-code-basic-lifecycle.json`,
      'config/parity-specs/discounts/discount-code-basic-lifecycle.json',
    ],
    cleanupBehavior: 'Deletes created code discount during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-code-basic-name-alias-independence',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-name-alias-independence-conformance.ts',
    purpose: 'Code discount basic create under an ordinary client operation name and aliased response key.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-code-basic-name-alias-independence.json`,
      'config/parity-specs/discounts/discount-code-basic-name-alias-independence.json',
      'config/parity-requests/discounts/discount-code-basic-name-alias-independence-create.graphql',
    ],
    cleanupBehavior: 'Creates one disposable code discount and deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-activate-deactivate-noop-idempotence',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-activate-deactivate-noop-idempotence-conformance.ts',
    purpose:
      'Code and automatic basic discount activate/deactivate no-op idempotence for already-active and already-expired records.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-activate-deactivate-noop-idempotence.json`,
      'config/parity-specs/discounts/discount-activate-deactivate-noop-idempotence.json',
      'config/parity-requests/discounts/discount-activate-deactivate-noop-automatic-activate.graphql',
      'config/parity-requests/discounts/discount-activate-deactivate-noop-automatic-deactivate.graphql',
      'config/parity-requests/discounts/discount-activate-deactivate-noop-code-activate.graphql',
      'config/parity-requests/discounts/discount-activate-deactivate-noop-code-deactivate.graphql',
    ],
    cleanupBehavior:
      'Creates disposable active and expired code/automatic basic discounts, captures no-op transitions, records hydrate cassette entries, and deletes all created discounts.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-activate-deactivate-edge-cases',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-activate-deactivate-edge-cases-conformance.ts',
    purpose:
      'Code discount activate/deactivate timestamp rewrites plus code and automatic unknown-id INVALID userErrors for all activate/deactivate roots.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-activate-deactivate-edge-cases.json`,
      'config/parity-specs/discounts/discount-activate-deactivate-edge-cases.json',
      'config/parity-requests/discounts/discount-activate-deactivate-edge-activate.graphql',
      'config/parity-requests/discounts/discount-activate-deactivate-edge-create.graphql',
      'config/parity-requests/discounts/discount-activate-deactivate-edge-deactivate.graphql',
      'config/parity-requests/discounts/discount-activate-deactivate-edge-read.graphql',
      'config/parity-requests/discounts/discount-activate-deactivate-edge-unknown.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable scheduled code basic discount, captures status transitions and unknown-id failures, then deletes the setup discount during the scenario with finally-block cleanup on failure.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-activation-failure-field-base-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-activation-failure-field-base-local-runtime.ts',
    purpose:
      'Local-runtime recording for app discount activation failure after a staged discount Function becomes unavailable.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [`${LOCAL_RUNTIME_ROOT}discount-activation-failure-field-base.json`],
    cleanupBehavior:
      'Runs only against the local proxy runtime with a deterministic Function cassette; no Shopify cleanup required.',
    expectedStatusChecks: ['conformance:check', 'rust:test', 'targeted-runtime-test'],
  },
  {
    domain: 'discounts',
    captureId: 'discount-numeric-bounds',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-numeric-bounds-conformance.ts',
    purpose:
      'Discount usageLimit, recurringCycleLimit, and fixed amount numeric bounds for basic code/automatic create and update inputs.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-numeric-bounds.json`,
      'config/parity-specs/discounts/discount-numeric-bounds.json',
      'config/parity-requests/discounts/discount-numeric-bounds-setup.graphql',
      'config/parity-requests/discounts/discount-numeric-bounds-code-basic-create.graphql',
      'config/parity-requests/discounts/discount-numeric-bounds-code-basic-update.graphql',
      'config/parity-requests/discounts/discount-numeric-bounds-automatic-basic-create.graphql',
      'config/parity-requests/discounts/discount-numeric-bounds-automatic-basic-update.graphql',
      'config/parity-requests/discounts/discount-numeric-bounds-recurring-float-variable.graphql',
      'config/parity-requests/discounts/discount-numeric-bounds-recurring-float-literal.graphql',
    ],
    cleanupBehavior:
      'Creates disposable code and automatic basic discounts for update validation, captures rejected validation branches, then deletes setup discounts.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-automatic-value-bounds',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-automatic-value-bounds-conformance.ts',
    purpose:
      'Automatic basic customerGets value bounds for zero, negative, and over-range percentage/fixed-amount create and update inputs.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-automatic-value-bounds.json`,
      'config/parity-specs/discounts/discount-automatic-value-bounds.json',
      'config/parity-requests/discounts/discount-automatic-value-bounds-setup.graphql',
      'config/parity-requests/discounts/discount-automatic-value-bounds-create.graphql',
      'config/parity-requests/discounts/discount-automatic-value-bounds-update.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable automatic basic discount for update validation, captures rejected value-bound branches, and deletes setup plus any unexpectedly created discounts.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-buyer-context',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-buyer-context-conformance.ts',
    purpose: 'Code and automatic basic discount customer/segment buyer context lifecycle behavior.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-buyer-context-lifecycle.json`,
      'config/parity-requests/discounts/discount-context-segment-hydrate.graphql',
    ],
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
    captureId: 'discount-bxgy-numeric-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-bxgy-numeric-validation-conformance.ts',
    purpose:
      'Buy-X-get-Y usesPerOrderLimit and quantity numeric validation for code and automatic create/update, plus captured ratio acceptance.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-bxgy-numeric-validation.json`,
      'config/parity-specs/discounts/discount-bxgy-numeric-validation.json',
      'config/parity-requests/discounts/discount-bxgy-numeric-validation-automatic-create.graphql',
      'config/parity-requests/discounts/discount-bxgy-numeric-validation-automatic-update.graphql',
      'config/parity-requests/discounts/discount-bxgy-numeric-validation-code-create.graphql',
      'config/parity-requests/discounts/discount-bxgy-numeric-validation-code-update.graphql',
    ],
    cleanupBehavior: 'Creates temporary products and setup BXGY discounts, then deletes captured discounts/products.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-items-refs-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-items-refs-validation-conformance.ts',
    purpose:
      'Discount customerGets/customerBuys product, variant, and collection reference validation guardrails and success branches.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_products', 'write_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-items-refs-validation.json`,
      'config/parity-requests/discounts/discount-item-refs-hydrate.graphql',
    ],
    cleanupBehavior: 'Deletes temporary discounts, products, and collection after capture.',
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
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-free-shipping-lifecycle.json`,
      'config/parity-specs/discounts/discount-free-shipping-lifecycle.json',
    ],
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
      'config/parity-requests/discounts/discount-redeem-code-bulk-delete-setup.graphql',
      'config/parity-requests/discounts/discount-redeem-code-bulk-live-add.graphql',
      'config/parity-requests/discounts/discount-redeem-code-bulk-live-delete.graphql',
      'config/parity-requests/discounts/discount-redeem-code-bulk-live-read.graphql',
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
      'config/parity-requests/discounts/discount-redeem-code-bulk-validation-existing-read.graphql',
      'config/parity-requests/discounts/discount-redeem-code-bulk-validation-read.graphql',
    ],
    cleanupBehavior: 'Creates two disposable code discounts and deletes them after validation probes.',
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
      'config/parity-requests/discounts/discount-uniqueness-check.graphql',
    ],
    cleanupBehavior: 'Validation-oriented; deletes any created disposable discount artifacts.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-starts-at-required-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-starts-at-required-validation-conformance.ts',
    purpose: 'Native discount create startsAt presence validation for all six native create roots.',
    requiredAuthScopes: ['read_discounts', 'write_discounts', 'read_products'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-starts-at-required-validation.json`,
      'config/parity-specs/discounts/discount-starts-at-required-validation.json',
      'config/parity-requests/discounts/discount-starts-at-required-validation.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture; creates no discounts when Shopify rejects missing or blank startsAt, with defensive cleanup if a root unexpectedly creates one.',
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
      'config/parity-specs/discounts/discount-app-bulk-local-runtime.json',
      `${LOCAL_RUNTIME_ROOT}discount-app-bulk-local-runtime.json`,
      `${CAPTURE_ROOT}discount-app-function-validation.json`,
      'config/parity-specs/discounts/discount-app-function-validation.json',
      'config/parity-requests/discounts/discount-app-function-validation.graphql',
      'config/parity-requests/discounts/discount-bulk-local-runtime-preconditions.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no discounts are created on successful capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'app-discount-input-validator',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-app-input-validator-conformance.ts',
    purpose:
      'App-discount create/update input validator userErrors for blank code/title, missing startsAt on create, empty discountClasses, and empty customer/segment selections.',
    requiredAuthScopes: [
      'read_discounts',
      'write_discounts',
      'shopifyFunctions read access',
      'released discount Shopify Function in the installed conformance app',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}app-discount-input-validator.json`,
      'config/parity-specs/discounts/app-discount-input-validator.json',
      'config/parity-requests/discounts/app-discount-input-validator-setup.graphql',
      'config/parity-requests/discounts/app-discount-input-validator-code-create.graphql',
      'config/parity-requests/discounts/app-discount-input-validator-automatic-create.graphql',
      'config/parity-requests/discounts/app-discount-input-validator-code-update.graphql',
      'config/parity-requests/discounts/app-discount-input-validator-automatic-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable app-managed code and automatic discounts, captures validation failures plus a combinesWith acceptance probe, and deletes all created discounts in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-title-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-title-validation-conformance.ts',
    purpose:
      'Discount title blank and overlong userErrors across code, automatic, and app-managed create/update roots.',
    requiredAuthScopes: [
      'read_discounts',
      'write_discounts',
      'shopifyFunctions read access',
      'released discount Shopify Function in the installed conformance app',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-title-validation.json`,
      'config/parity-specs/discounts/discount-title-validation.json',
      'config/parity-requests/discounts/discount-title-validation-setup.graphql',
      'config/parity-requests/discounts/discount-title-validation-create.graphql',
      'config/parity-requests/discounts/discount-title-validation-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable code, automatic, and app-managed discounts for update IDs, captures blank-title and 256-character title validation failures, and deletes every created discount in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-code-app-title',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-code-app-title-conformance.ts',
    purpose:
      'App-managed code discount title validation for blank, omitted, and overlong titles, with automatic-app blank-title validation as the control branch.',
    requiredAuthScopes: [
      'read_discounts',
      'write_discounts',
      'shopifyFunctions read access',
      'released discount Shopify Function in the installed conformance app',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-code-app-title.json`,
      'config/parity-specs/discounts/discount-code-app-title.json',
      'config/parity-requests/discounts/discount-code-app-title-setup.graphql',
      'config/parity-requests/discounts/discount-code-app-title-create.graphql',
      'config/parity-requests/discounts/discount-code-app-title-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable app-managed code and automatic discounts, captures code-app title validation plus automatic blank-title validation, and deletes all created discounts in cleanup.',
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
    captureId: 'discount-bulk-search-field-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-bulk-search-field-validation-conformance.ts',
    purpose:
      'Discount bulk search field-name validation for code and automatic bulk roots, including root-specific acceptance of code-specific fields.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-bulk-search-field-validation.json`,
      'config/parity-specs/discounts/discount-bulk-search-field-validation.json',
      'config/parity-requests/discounts/discount-bulk-search-field-validation.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no discounts are created on successful capture.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'discounts',
    captureId: 'discount-bulk-search-effects',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-bulk-search-effects-conformance.ts',
    purpose:
      'Search-selector local effects for broad discount bulk activate, deactivate, code delete, and automatic delete roots.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-bulk-search-effects.json`,
      'config/parity-specs/discounts/discount-bulk-search-effects.json',
      'config/parity-requests/discounts/discount-bulk-search-effects-setup.graphql',
      'config/parity-requests/discounts/discount-bulk-search-effects-create-code.graphql',
      'config/parity-requests/discounts/discount-bulk-search-effects-create-code-and-automatic.graphql',
      'config/parity-requests/discounts/discount-bulk-search-effects-create-automatic.graphql',
      'config/parity-requests/discounts/discount-bulk-search-effects-activate-read.graphql',
      'config/parity-requests/discounts/discount-bulk-search-effects-code-activate.graphql',
      'config/parity-requests/discounts/discount-bulk-search-effects-code-deactivate.graphql',
      'config/parity-requests/discounts/discount-bulk-search-effects-code-delete.graphql',
      'config/parity-requests/discounts/discount-bulk-search-effects-automatic-delete.graphql',
      'config/parity-requests/discounts/discount-bulk-search-effects-read-code.graphql',
      'config/parity-requests/discounts/discount-bulk-search-effects-read-code-delete.graphql',
      'config/parity-requests/discounts/discount-bulk-search-effects-read-automatic.graphql',
    ],
    cleanupBehavior:
      'Creates disposable code and automatic discounts with unique titles, captures search-based bulk effects and read-after-write state, then deletes remaining created discounts in cleanup.',
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
    captureId: 'discount-amount-applies-on-each-item',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-discount-amount-applies-on-each-item-conformance.ts',
    purpose:
      'Fixed DiscountAmount appliesOnEachItem read-after-write for code-basic and automatic-basic discounts, plus public-schema deprecated field rejection.',
    requiredAuthScopes: ['read_discounts', 'write_discounts'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}discount-amount-applies-on-each-item.json`,
      'config/parity-specs/discounts/discount-amount-applies-on-each-item.json',
      'config/parity-requests/discounts/discount-amount-applies-on-each-item-product-setup.graphql',
      'config/parity-requests/discounts/discount-amount-applies-on-each-item-code-create.graphql',
      'config/parity-requests/discounts/discount-amount-applies-on-each-item-read.graphql',
      'config/parity-requests/discounts/discount-amount-applies-on-each-item-automatic-create.graphql',
      'config/parity-requests/discounts/discount-amount-applies-on-each-item-automatic-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable code and automatic basic discounts, captures fixed-amount readback and schema rejection branches, then deletes created discounts.',
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
    captureId: 'app-revoke-access-scopes-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-app-revoke-access-scopes-validation-conformance.ts',
    purpose: 'Safe appRevokeAccessScopes validation branches that do not revoke real app grants.',
    requiredAuthScopes: ['active Admin API token with current app source context'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}app-revoke-access-scopes-validation.json`,
      'config/parity-specs/apps/app-revoke-access-scopes-validation.json',
      'config/parity-requests/apps/appRevokeAccessScopes-fake-scope.graphql',
      'config/parity-requests/apps/appRevokeAccessScopes-mixed-fake-scope.graphql',
      'config/parity-requests/apps/appRevokeAccessScopes-required-read-products.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture; unknown and required-scope probes do not revoke app grants. Optional-grant success is intentionally excluded because it would revoke a real active-app scope.',
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
    domain: 'events',
    captureId: 'platform-payments-orphaned-fixtures-events',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01', ORPHAN_FIXTURE_GROUP: 'events' },
    scriptPath: 'scripts/capture-platform-payments-orphaned-fixtures-conformance.ts',
    purpose: 'Re-records the event empty-read fixture that is consumed by the standard parity runner.',
    requiredAuthScopes: ['active Admin API token'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/events/event-empty-read.json',
      'config/parity-specs/events/event-empty-read.json',
      'config/parity-requests/events/event-empty-read.graphql',
      'config/parity-requests/events/event-empty-read.variables.json',
    ],
    cleanupBehavior: 'Read-only capture; no Shopify resources are created.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'apps',
    captureId: 'platform-payments-orphaned-fixtures-apps',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04', ORPHAN_FIXTURE_GROUP: 'apps' },
    scriptPath: 'scripts/capture-platform-payments-orphaned-fixtures-conformance.ts',
    purpose:
      'Re-records delegate access token create validation and destroy code fixtures consumed by the standard parity runner.',
    requiredAuthScopes: ['delegate access token create/destroy for the installed app'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/apps/delegate-access-token-create-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/apps/delegate-access-token-create-expires-after-parent.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/apps/delegate-access-token-destroy-codes.json',
      'config/parity-specs/apps/delegate-access-token-create-validation.json',
      'config/parity-specs/apps/delegate-access-token-create-expires-after-parent.json',
      'config/parity-specs/apps/delegate-access-token-destroy-codes.json',
      'config/parity-requests/apps/delegateAccessTokenCreate-current-input-local-lifecycle.graphql',
      'config/parity-requests/apps/delegateAccessTokenCreate-empty-scope-validation.graphql',
      'config/parity-requests/apps/delegateAccessTokenCreate-expires-after-parent-anonymous.graphql',
      'config/parity-requests/apps/delegateAccessTokenCreate-expires-after-parent.graphql',
      'config/parity-requests/apps/delegateAccessTokenCreate-expires-after-parent-ordinary-operation-name.graphql',
      'config/parity-requests/apps/delegateAccessTokenCreate-happy-validation.graphql',
      'config/parity-requests/apps/delegateAccessTokenCreate-negative-expires-validation.graphql',
      'config/parity-requests/apps/delegateAccessTokenCreate-ordinary-operation-name.graphql',
      'config/parity-requests/apps/delegateAccessTokenCreate-unknown-scope-validation.graphql',
      'config/parity-requests/apps/delegateAccessTokenDestroy-codes.graphql',
    ],
    cleanupBehavior:
      'Creates short-lived delegate access tokens for success and hierarchy probes and destroys them during the scenario.',
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
    captureId: 'functions-non-catalog-hydrate',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-non-catalog-hydrate-conformance.ts',
    purpose:
      'validationCreate wrong-API evidence for a released ShopifyFunction id outside the removed local Functions catalog, replayed through the FunctionHydrateById upstreamCalls cassette.',
    requiredAuthScopes: ['shopifyFunctions read access', 'write_validations for validationCreate userError branch'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-non-catalog-hydrate-validation-create.json`,
      'config/parity-specs/functions/functions-non-catalog-hydrate-validation-create.json',
      'config/parity-requests/functions/functions-non-catalog-hydrate-validation-create.graphql',
    ],
    cleanupBehavior:
      'Captures a wrong-API validationCreate userError for a non-validation Function; no validation is created and no cleanup resource is expected.',
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
    captureId: 'functions-cart-transform-registered-wrong-api-precedence',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-cart-transform-registered-wrong-api-precedence-conformance.ts',
    purpose:
      'cartTransformCreate userError precedence when a validation Function is already registered on the shop before the cart-transform API mismatch check.',
    requiredAuthScopes: [
      'shopifyFunctions read access',
      'write_validations for disposable validation setup and cleanup',
      'read_cart_transforms',
      'write_cart_transforms for cartTransformCreate userError probes',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-cart-transform-create-registered-wrong-api-precedence.json`,
      'config/parity-specs/functions/functions-cart-transform-create-registered-wrong-api-precedence.json',
      'config/parity-requests/functions/functions-cart-transform-create-registered-wrong-api-validation-setup.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-registered-wrong-api-by-id.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-registered-wrong-api-by-handle.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable Validation from the validation Function, captures cartTransformCreate functionId/functionHandle probes against that registered wrong-API Function, then deletes the disposable Validation.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'functions',
    captureId: 'functions-cart-transform-create-metafields',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-cart-transform-metafields-conformance.ts',
    purpose:
      'cartTransformCreate metafield input validation, direct top-level argument shape, valid metafield persistence, downstream cartTransforms metafield readback, and invalid CartTransform field/input rejection.',
    requiredAuthScopes: [
      'shopifyFunctions read access',
      'read_cart_transforms',
      'write_cart_transforms for disposable cart-transform create/delete and cleanup',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-cart-transform-create-metafields.json`,
      'config/parity-specs/functions/functions-cart-transform-create-metafields.json',
      'config/parity-requests/functions/functions-cart-transform-create-metafields-invalid.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-metafields-success.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-metafields-read.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-shape-invalid-fields.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-shape-invalid-wrapper.graphql',
    ],
    cleanupBehavior:
      'Deletes pre-existing cartTransforms, captures invalid metafield branches without side effects, creates one disposable cartTransform with two metafields, captures downstream readback, then deletes the disposable cartTransform.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'functions',
    captureId: 'platform-payments-orphaned-fixtures-functions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04', ORPHAN_FIXTURE_GROUP: 'functions' },
    scriptPath: 'scripts/capture-platform-payments-orphaned-fixtures-conformance.ts',
    purpose:
      'Re-records cartTransformCreate validation plus local-runtime Function validation/update fixtures consumed by the standard parity runner.',
    requiredAuthScopes: [
      'shopifyFunctions read access',
      'read_cart_transforms',
      'write_cart_transforms for cleanup of disposable cart transforms',
    ],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/functions/functions-cart-transform-create-validation.json',
      `${LOCAL_RUNTIME_ROOT}functions-metadata-flow.json`,
      `${LOCAL_RUNTIME_ROOT}functions-owner-metadata-flow.json`,
      `${LOCAL_RUNTIME_ROOT}functions-validation-create-validation.json`,
      `${LOCAL_RUNTIME_ROOT}functions-create-guardrails.json`,
      'config/parity-specs/functions/functions-cart-transform-create-validation.json',
      'config/parity-specs/functions/functions-create-guardrails.json',
      'config/parity-specs/functions/functions-validation-create-validation.json',
      'config/parity-specs/functions/functions-validation-max-cap.json',
      'config/parity-specs/functions/functions-validation-update-shape.json',
      'config/parity-requests/functions/functions-create-guardrails.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-validation-api-mismatch.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-validation-both.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-validation-conflict.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-validation-read.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-validation-setup.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-validation-unknown-handle.graphql',
      'config/parity-requests/functions/functions-cart-transform-create-validation-unknown-id.graphql',
      'config/parity-requests/functions/functions-metadata-read.graphql',
      'config/parity-requests/functions/functions-owner-metadata-read.graphql',
      'config/parity-requests/functions/functions-validation-create-validation-read.graphql',
    ],
    cleanupBehavior:
      'Deletes pre-existing cartTransforms before capture, captures unresolved identifier branches with empty readbacks, creates one disposable cartTransform, captures duplicate/API-mismatch/both-identifier branches and downstream readback, then deletes the disposable cartTransform.',
    notes:
      'The functions-create-guardrails protected outputs are retained here only to register their deletion with the protected-evidence invariant; the fabricated local-runtime scenario is no longer generated or checked.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'functions',
    captureId: 'functions-validation-update-rebind-variable',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-validation-update-rebind-variable-conformance.ts',
    purpose:
      'validationUpdate variable-bound functionId/functionHandle rebind input is rejected by GraphQL variable coercion before resolver execution.',
    requiredAuthScopes: ['write_validations schema access; request is rejected before resolver execution'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-validation-update-rebind-variable.json`,
      'config/parity-specs/functions/functions-validation-update-rebind-variable.json',
      'config/parity-requests/functions/functions-validation-update-rebind-variable.graphql',
    ],
    cleanupBehavior:
      'No resources are created or mutated; both requests contain invalid ValidationUpdateInput fields and are rejected before validationUpdate resolver execution.',
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
    captureId: 'functions-fulfillment-constraint-rule-errors',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-fulfillment-constraint-rule-errors-conformance.ts',
    purpose:
      'fulfillmentConstraintRuleCreate deterministic missing/multiple/empty-delivery/unknown-function userErrors, fulfillmentConstraintRuleDelete unknown-id shape, and empty fulfillmentConstraintRules read.',
    requiredAuthScopes: [
      'read_fulfillment_constraint_rules for fulfillmentConstraintRules empty read',
      'write_fulfillment_constraint_rules for create/delete userError capture',
      'released fulfillment-constraint Function required only for future success-path and wrong-API-type capture',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-fulfillment-constraint-rule-errors.json`,
      'config/parity-specs/functions/functions-fulfillment-constraint-rule-errors.json',
      'config/parity-requests/functions/functions-fulfillment-constraint-rule-errors.graphql',
      'config/parity-requests/functions/functions-fulfillment-constraint-rule-unknown-function.graphql',
      'config/parity-requests/functions/functions-fulfillment-constraint-rules-empty-read.graphql',
    ],
    cleanupBehavior:
      'Captures deterministic userErrors and an empty read only; no live fulfillment constraint rule is created.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'functions',
    captureId: 'functions-output-field-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-output-field-validation-conformance.ts',
    purpose:
      'Validation and FulfillmentConstraintRule public output field sets plus undefined-field rejection for local-only fabricated fields.',
    requiredAuthScopes: [
      'read_validations',
      'write_validations for disposable validationCreate/delete lifecycle capture',
      'read_fulfillment_constraint_rules for valid empty fulfillmentConstraintRules read',
      'released conformance-validation Function in the installed conformance app',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-output-field-validation.json`,
      'config/parity-specs/functions/functions-output-field-validation.json',
      'config/parity-requests/functions/functions-output-field-validation-create.graphql',
      'config/parity-requests/functions/functions-output-field-validation-validation-invalid-read.graphql',
      'config/parity-requests/functions/functions-output-field-validation-validations-invalid-read.graphql',
      'config/parity-requests/functions/functions-output-field-validation-validation-valid-read.graphql',
      'config/parity-requests/functions/functions-output-field-validation-validation-node-invalid-read.graphql',
      'config/parity-requests/functions/functions-output-field-validation-validation-node-valid-read.graphql',
      'config/parity-requests/functions/functions-output-field-validation-fcr-invalid-read.graphql',
      'config/parity-requests/functions/functions-output-field-validation-fcr-node-invalid-read.graphql',
      'config/parity-requests/functions/functions-output-field-validation-fcr-valid-empty-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable Validation through conformance-validation, captures invalid and valid Validation/FulfillmentConstraintRule reads, then deletes the disposable Validation. FulfillmentConstraintRule negative selection validation does not create a rule.',
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
      `${LOCAL_RUNTIME_ROOT}functions-validation-update-shape.json`,
      `${CAPTURE_ROOT}functions-validation-update-defaults.json`,
      'config/parity-specs/functions/functions-validation-update-defaults.json',
    ],
    cleanupBehavior:
      'Creates one disposable validation through conformance-validation, updates it, reads it back, captures the missing-id branch, then deletes the disposable validation.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'functions',
    captureId: 'functions-validation-metafields-input-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-validation-metafields-input-validation-conformance.ts',
    purpose:
      'validationCreate and validationUpdate metafields input validation userError shapes and atomic no-write behavior.',
    requiredAuthScopes: [
      'read_validations',
      'write_validations for disposable validationCreate/update/delete lifecycle capture',
      'released conformance-validation Function in the installed conformance app',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-validation-metafields-input-validation.json`,
      'config/parity-specs/functions/functions-validation-metafields-input-validation.json',
    ],
    cleanupBehavior:
      'Creates one disposable validation with a valid metafield, captures invalid validationUpdate no-write behavior, captures validationCreate invalid metafield branches, and deletes the disposable validation.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'functions',
    captureId: 'functions-validation-update-metafields-upsert',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-validation-update-metafields-upsert-conformance.ts',
    purpose:
      'validationUpdate omitted, empty, and partial non-empty metafields input semantics for validation-owned metafield rows.',
    requiredAuthScopes: [
      'read_validations',
      'write_validations for disposable validationCreate/update/delete lifecycle capture',
      'released conformance-validation Function in the installed conformance app',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}functions-validation-update-metafields-upsert.json`,
      'config/parity-specs/functions/functions-validation-update-metafields-upsert.json',
      'config/parity-requests/functions/functions-validation-update-metafields-upsert-create.graphql',
      'config/parity-requests/functions/functions-validation-update-metafields-upsert-update.graphql',
      'config/parity-requests/functions/functions-validation-update-metafields-upsert-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable validation with two metafields through conformance-validation, updates it with title-only, empty metafields, and partial metafields inputs, reads after each update, then deletes the disposable validation.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'functions',
    captureId: 'functions-validation-create-title-fallback',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-functions-validation-create-title-fallback-conformance.ts',
    purpose:
      'validationCreate omitted/null title fallback to the resolved ShopifyFunction title plus explicit empty-string preservation and downstream title reads.',
    requiredAuthScopes: [
      'read_validations',
      'write_validations for disposable validationCreate/delete lifecycle capture',
      'released conformance-validation Function in the installed conformance app',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}validation-create-title-fallback-parity.json`,
      'config/parity-specs/functions/validation-create-title-fallback-parity.json',
      'config/parity-requests/functions/validation-create-title-fallback-stage.graphql',
      'config/parity-requests/functions/validation-create-title-fallback-validation-read.graphql',
      'config/parity-requests/functions/validation-create-title-fallback-validations-read.graphql',
    ],
    cleanupBehavior:
      'Deletes disposable validations before capture, creates three validationCreate title cases through conformance-validation, verifies validation(id:) and validations(first: 3) title readback, then deletes the created validations.',
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
    domain: 'orders',
    captureId: 'order-payment-local-runtime-robustness',
    scriptPath: 'scripts/capture-transaction-void-codes-conformance.ts',
    purpose:
      'Local-runtime order payment parity robustness coverage that replays captured order payment staging evidence through unrelated client operation names.',
    requiredAuthScopes: ['local-runtime fixture evidence; no live Shopify write required'],
    fixtureOutputs: [
      'config/parity-specs/orders/order-payment-transaction-local-staging.json',
      'config/parity-specs/orders/order-payment-transaction-non-recording-operation-name.json',
      'config/parity-specs/orders/order-payment-transaction-void-local-staging.json',
      'config/parity-requests/orders/order-payment-non-recording-capture.graphql',
      'config/parity-requests/orders/order-payment-non-recording-create.graphql',
      'config/parity-requests/orders/order-payment-non-recording-mandate.graphql',
      'config/parity-requests/orders/order-payment-non-recording-read.graphql',
      'config/parity-requests/orders/order-payment-non-recording-void.graphql',
    ],
    cleanupBehavior: 'Local-runtime parity only; no live Shopify objects are created.',
    expectedStatusChecks: ['conformance:status', 'conformance:check', 'conformance:parity', 'rust:test'],
  },
  {
    domain: 'payments',
    captureId: 'order-capture-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-capture-validation-conformance.ts',
    purpose:
      'orderCapture multi-currency currency validation, single-currency omitted-currency and zero-amount behavior, missing parent transaction, invalid amount, over-capture, public manual-gateway finalCapture rejection, and follow-up capture behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}order-capture-validation.json`,
      'config/parity-specs/payments/order_capture_validation.json',
      'config/parity-requests/payments/order-capture-validation-order-capture.graphql',
      'config/parity-requests/payments/order-capture-validation-order-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable multi-currency authorization order and two disposable single-currency authorization orders, records validation and capture branches, then cancels the orders during cleanup.',
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
    captureId: 'payment-reminder-send-additional-guards',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-payment-reminder-additional-guards-conformance.ts',
    purpose:
      'Records public Admin-reproducible paymentReminderSend guard branches for blank order email and one reminder per order per 24 hours.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-additional-guards.json',
      'config/parity-specs/payments/payment-reminder-send-additional-guards.json',
      'config/parity-requests/payments/payment-reminder-send.graphql',
    ],
    cleanupBehavior:
      'Creates disposable draft/order/payment-terms records, sends one customer-visible reminder for the rate-limit baseline, captures the immediate second-send rejection, and cancels the completed orders during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Selling-plan, capture-at-fulfillment, and unsent PaymentCollection reminder guards depend on internal order/payment state that this public conformance harness cannot currently construct; runtime tests cover those local guardrails with explicit order-side state hints.',
  },
  {
    domain: 'payments',
    captureId: 'payment-reminder-send-malformed-gid',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-payment-reminder-malformed-gid-conformance.ts',
    purpose:
      'Records paymentReminderSend paymentScheduleId GID coercion errors for empty, non-GID, and wrong-resource GID variables.',
    requiredAuthScopes: ['read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-malformed-gid.json',
      'config/parity-specs/payments/payment-reminder-send-malformed-gid.json',
      'config/parity-requests/payments/payment-reminder-send-malformed-gid.graphql',
    ],
    cleanupBehavior: 'No setup or cleanup; the capture sends validation-only malformed IDs.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'platform-payments-orphaned-fixtures-payments',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01', ORPHAN_FIXTURE_GROUP: 'payments' },
    scriptPath: 'scripts/capture-platform-payments-orphaned-fixtures-conformance.ts',
    purpose:
      'Re-records payment customization empty/validation, payment reminder eligibility, payment terms template, and Shopify Payments account access fixtures consumed by the standard parity runner.',
    requiredAuthScopes: [
      'read_payment_customizations',
      'write_payment_customizations',
      'read_payment_terms',
      'write_payment_terms',
      'read_orders',
      'write_orders',
    ],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}customer-payment-method-local-staging.json`,
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-customization-empty-read.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-customization-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-reminder-send-eligibility.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/payment-terms-templates-read.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/payments/shopify-payments-account-access-denied.json',
      'config/parity-specs/payments/payment-customization-empty-read.json',
      'config/parity-specs/payments/payment-customization-validation.json',
      'config/parity-specs/payments/payment-reminder-send-eligibility.json',
      'config/parity-specs/payments/payment-terms-templates-read.json',
      'config/parity-specs/payments/shopify-payments-account-read.json',
      'config/parity-requests/payments/payment-customization-empty-read.graphql',
      'config/parity-requests/payments/payment-customization-validation.graphql',
      'config/parity-requests/payments/payment-reminder-send.graphql',
      'config/parity-requests/payments/payment-terms-templates-read.graphql',
      'config/parity-requests/payments/shopify-payments-account-read.graphql',
    ],
    cleanupBehavior:
      'Read and validation captures do not create resources; payment reminder eligibility creates disposable draft/order/payment-terms records and cancels the completed orders during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'payment-terms-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-terms-lifecycle-conformance.ts',
    purpose: 'paymentTermsCreate/paymentTermsUpdate/paymentTermsDelete lifecycle against a disposable draft order.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      'config/parity-specs/payments/payment-terms-create-on-order.json',
      'config/parity-specs/payments/payment_terms_delete_owner_cascade.json',
      `${LOCAL_RUNTIME_ROOT}payment-terms-create-on-order.json`,
      `${LOCAL_RUNTIME_ROOT}payment-terms-delete-owner-cascade.json`,
      `${CAPTURE_ROOT}payment-terms-lifecycle.json`,
      'config/parity-specs/payments/payment-terms-update-missing-local-runtime.json',
      'config/parity-requests/payments/payment-terms-update-missing-local-runtime.graphql',
      'config/parity-specs/payments/payment-terms-lifecycle-local-staging.json',
    ],
    cleanupBehavior:
      'Creates a disposable draft order, deletes payment terms during the scenario, then deletes the draft order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'payment-terms-due-state',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-terms-due-state-conformance.ts',
    purpose:
      'paymentTermsCreate/paymentTermsUpdate due and overdue booleans for past-due and future-due fixed schedules.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-terms-due-state.json`,
      'config/parity-specs/payments/payment-terms-due-state.json',
      'config/parity-requests/payments/payment-terms-due-state-draft-create.graphql',
      'config/parity-requests/payments/payment-terms-due-state-create.graphql',
      'config/parity-requests/payments/payment-terms-due-state-update.graphql',
      'config/parity-requests/payments/payment-terms-due-state-draft-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable draft orders with fixed payment terms, records past/future due-state create/update/readback behavior, deletes payment terms, then deletes the draft orders.',
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
    captureId: 'payment-terms-create-missing-template-id',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-terms-missing-template-id-conformance.ts',
    purpose:
      'paymentTermsCreate omitted paymentTermsTemplateId GraphQL variable coercion and paymentTermsUpdate omitted template success behavior.',
    requiredAuthScopes: ['read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-terms-create-missing-template-id.json`,
      'config/parity-specs/payments/payment-terms-create-missing-template-id.json',
      'config/parity-requests/payments/payment-terms-create-missing-template-id.graphql',
      'config/parity-requests/payments/payment-terms-update-missing-template-id-setup.graphql',
      'config/parity-requests/payments/payment-terms-update-missing-template-id.graphql',
    ],
    cleanupBehavior:
      'Create omission is validation-only. Update omission creates a disposable draft order and Net 30 payment terms, captures update success, then deletes both resources.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'payment-terms-create-template-reprojection',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-terms-template-reprojection-conformance.ts',
    purpose: 'paymentTermsCreate successful template reprojection for FIXED, non-30 NET, and FULFILLMENT templates.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-terms-create-template-reprojection.json`,
      'config/parity-specs/payments/payment-terms-create-template-reprojection.json',
      'config/parity-requests/payments/payment-terms-template-reprojection-order-create.graphql',
      'config/parity-requests/payments/payment-terms-create-template-reprojection.graphql',
    ],
    cleanupBehavior:
      'Creates disposable Orders, records one successful paymentTermsCreate per target template, deletes payment terms, then cancels every Order.',
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
    captureId: 'payment-terms-update-order-eligibility',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-terms-update-order-eligibility-conformance.ts',
    purpose:
      'paymentTermsUpdate Order eligibility rejection after an existing payment terms owner Order is marked paid.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-terms-update-order-eligibility.json`,
      `${CAPTURE_ROOT}payment-terms-update-order-eligibility-cleanup.json`,
      'config/parity-specs/payments/payment-terms-update-order-eligibility.json',
      'config/parity-requests/payments/payment-terms-update-order-eligibility.graphql',
    ],
    cleanupBehavior:
      'Creates a disposable unpaid test Order, creates payment terms, marks the Order paid, captures the rejected paymentTermsUpdate payload, then best-effort deletes payment terms and cancels the Order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Channel-policy-disallowed update evidence remains fixture-conditional until a captureable sales-channel setup path is available.',
  },
  {
    domain: 'payments',
    captureId: 'payment-terms-create-reference-not-found',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-terms-reference-not-found-conformance.ts',
    purpose:
      'paymentTermsCreate unknown Order and DraftOrder reference userError field paths and type-specific messages.',
    requiredAuthScopes: ['read_orders', 'read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-terms-create-reference-not-found.json`,
      'config/parity-specs/payments/payment-terms-create-reference-not-found.json',
      'config/parity-requests/payments/payment-terms-create-reference-not-found.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; creates no Shopify resources.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'payment-terms-delete-not-found',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-terms-delete-not-found-conformance.ts',
    purpose: 'paymentTermsDelete unknown PaymentTerms id userError field, message, and public enum code.',
    requiredAuthScopes: ['read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-terms-delete-not-found.json`,
      'config/parity-specs/payments/payment-terms-delete-not-found.json',
      'config/parity-requests/payments/payment-terms-delete-not-found.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; creates no Shopify resources.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'payment-terms-multiple-schedules',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-terms-multiple-schedules-conformance.ts',
    purpose: 'paymentTermsCreate and paymentTermsUpdate multiple paymentSchedules userError field, message, and code.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_payment_terms', 'write_payment_terms'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-terms-multiple-schedules.json`,
      'config/parity-specs/payments/payment-terms-multiple-schedules.json',
      'config/parity-requests/payments/payment-terms-multiple-schedules-setup.graphql',
      'config/parity-requests/payments/payment-terms-multiple-schedules-create.graphql',
      'config/parity-requests/payments/payment-terms-multiple-schedules-update.graphql',
    ],
    cleanupBehavior:
      'Creates a disposable draft order, captures create rejection, creates one valid payment terms record, captures update rejection, then deletes payment terms and draft order.',
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
      'config/parity-requests/payments/payment-customization-invalid-metafields-create.graphql',
      'config/parity-requests/payments/payment-customization-invalid-metafields-update.graphql',
      'config/parity-requests/payments/payment-customization-metafields-create.graphql',
      'config/parity-requests/payments/payment-customization-metafields-create-local-runtime.graphql',
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
    captureId: 'payment-customization-activation-mixed',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-customization-activation-mixed-conformance.ts',
    purpose:
      'paymentCustomizationActivation mixed valid/missing id bucketing and filtered ids return payload against a disposable payment customization.',
    requiredAuthScopes: ['read_payment_customizations', 'write_payment_customizations', 'shopifyFunctions read access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-customization-activation-mixed.json`,
      'config/parity-specs/payments/payment-customization-activation-mixed.json',
      'config/parity-requests/payments/payment-customization-activation-mixed.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable payment customization, activates it with one valid id plus one known-missing id, then deletes the payment customization.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'payments',
    captureId: 'payment-customization-activation-already-in-state',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-payment-customization-activation-already-in-state-conformance.ts',
    purpose:
      'paymentCustomizationActivation returns a valid id when the submitted disposable payment customization is already in the requested enabled state.',
    requiredAuthScopes: ['read_payment_customizations', 'write_payment_customizations', 'shopifyFunctions read access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}payment-customization-activation-already-in-state.json`,
      'config/parity-specs/payments/payment-customization-activation-already-in-state.json',
      'config/parity-requests/payments/payment-customization-activation-already-in-state-create.graphql',
      'config/parity-requests/payments/payment-customization-activation-already-in-state.graphql',
    ],
    cleanupBehavior:
      'Creates one enabled disposable payment customization, re-activates it with enabled:true to capture the no-op success ids payload, then deletes the payment customization.',
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
      'config/parity-specs/payments/payment-customization-create-required-fields.json',
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
    purpose:
      'Admin platform utility roots, backupRegion current-region update/readback, MarketRegionCountry node readback, and staff/access blocker evidence.',
    requiredAuthScopes: ['active Admin API token; staff/utility roots may require plan or staff permissions'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}admin-platform-utility-roots.json`,
      `${CAPTURE_ROOT}admin-platform-taxonomy-hierarchy-node-reads.json`,
      'config/parity-specs/admin-platform/admin-platform-utility-reads.json',
      'config/parity-specs/admin-platform/admin-platform-backup-region-update.json',
      'config/parity-specs/admin-platform/admin-platform-job-arbitrary-gid.json',
      'config/parity-specs/admin-platform/admin-platform-market-region-node-read.json',
      'config/parity-specs/admin-platform/admin-platform-node-malformed-gid.json',
      'config/parity-requests/admin-platform/admin-platform-backup-region-read.graphql',
      'config/parity-requests/admin-platform/admin-platform-backup-region-update-idempotent.graphql',
      'config/parity-requests/admin-platform/admin-platform-backup-region-update-invalid.graphql',
      'config/parity-requests/admin-platform/admin-platform-job-arbitrary-gid.graphql',
      'config/parity-requests/admin-platform/admin-platform-market-region-node-read.graphql',
      'config/parity-requests/admin-platform/admin-platform-node-malformed-gid-node.graphql',
      'config/parity-requests/admin-platform/admin-platform-node-malformed-gid-nodes.graphql',
      'config/parity-specs/admin-platform/admin-platform-flow-trigger-receive-body-validation.json',
      'config/parity-specs/admin-platform/admin-platform-taxonomy-hierarchy-node-reads.json',
    ],
    cleanupBehavior:
      'Read-only, blocked-root, and idempotent current backup-region capture; no net Shopify state change or cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    captureId: 'admin-platform-flow-trigger-receive-body-schema-gaps',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-admin-platform-flow-trigger-body-schema-gaps-conformance.ts',
    purpose:
      'flowTriggerReceive body-only validation for missing trigger references, unknown trigger references, absolute resource URLs, unknown root fields, and accumulated schema errors.',
    requiredAuthScopes: [
      'active Admin API token; validation-only Flow trigger receive branches short-circuit before delivery',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}admin-platform-flow-trigger-receive-body-schema-gaps.json`,
      'config/parity-specs/admin-platform/admin-platform-flow-trigger-receive-body-schema-gaps.json',
      'config/parity-requests/admin-platform/admin-platform-flow-trigger-receive-body-schema-gaps.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no external Flow trigger delivery and no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    captureId: 'admin-platform-flow-trigger-receive-property-size-boundary',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-admin-platform-flow-trigger-property-size-boundary-conformance.ts',
    purpose:
      'flowTriggerReceive property byte-size validation for body-only bloated resources, oversized parsed properties, and near-limit handle payload branches.',
    requiredAuthScopes: [
      'active Admin API token; validation-only Flow trigger receive branches short-circuit before delivery',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}admin-platform-flow-trigger-receive-property-size-boundary.json`,
      'config/parity-specs/admin-platform/admin-platform-flow-trigger-receive-property-size-boundary.json',
      'config/parity-requests/admin-platform/admin-platform-flow-trigger-receive-property-size-boundary.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no external Flow trigger delivery and no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    captureId: 'admin-platform-by-id-not-found-read',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-admin-platform-conformance.mts',
    purpose:
      'Read-only singular id root not-found evidence for implemented admin-platform fetchers added after the original product/draft-order not-found pass.',
    requiredAuthScopes: ['active Admin API token with Admin GraphQL schema access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}by-id-not-found-read.json`,
      'config/parity-specs/admin-platform/by-id-not-found-read.json',
      'config/parity-requests/admin-platform/by-id-not-found/customers.graphql',
      'config/parity-requests/admin-platform/by-id-not-found/discounts.graphql',
      'config/parity-requests/admin-platform/by-id-not-found/functions.graphql',
      'config/parity-requests/admin-platform/by-id-not-found/gift-cards.graphql',
      'config/parity-requests/admin-platform/by-id-not-found/markets.graphql',
      'config/parity-requests/admin-platform/by-id-not-found/online-store.graphql',
      'config/parity-requests/admin-platform/by-id-not-found/orders.graphql',
      'config/parity-requests/admin-platform/by-id-not-found/products.graphql',
      'config/parity-requests/admin-platform/by-id-not-found/segments.graphql',
      'config/parity-requests/admin-platform/by-id-not-found/shipping-fulfillments.graphql',
      'config/parity-requests/admin-platform/by-id-not-found/store-properties.graphql',
    ],
    cleanupBehavior: 'Read-only missing-id probes; no Shopify state is created and no cleanup is expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    captureId: 'admin-platform-backup-region-update-extended',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-admin-platform-backup-region-update-extended.mts',
    purpose:
      'backupRegionUpdate omitted/null current-state semantics, AE/US/JP success, read-after-write, and REGION_NOT_FOUND validation.',
    requiredAuthScopes: ['active Admin API token with Markets/admin platform access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}admin-platform-backup-region-update-extended.json`,
      'config/parity-specs/admin-platform/admin-platform-backup-region-update-extended.json',
      'config/parity-requests/admin-platform/admin-platform-backup-region-update-us.graphql',
      'config/parity-requests/admin-platform/admin-platform-backup-region-update-jp.graphql',
    ],
    cleanupBehavior:
      'Temporarily stages CA, AE, US, and JP as the backup region; creates/deletes temporary region markets if needed; then restores the store backup region to its original country.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    captureId: 'domain-primary-domain-read',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2025-01' },
    scriptPath: 'scripts/capture-domain-primary-domain-read-conformance.mts',
    purpose:
      'Direct domain(id:) read evidence for a connected-shop primary domain id that is not the legacy local Domain/1000 id.',
    requiredAuthScopes: ['active Admin API token with shop/domain read access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}domain-primary-domain-read.json`,
      'config/parity-specs/admin-platform/domain-primary-domain-read.json',
      'config/parity-requests/admin-platform/domain-primary-domain-read.graphql',
      'config/parity-requests/admin-platform/domain-primary-domain-read.variables.json',
    ],
    cleanupBehavior: 'Read-only capture; no cleanup expected.',
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
    domain: 'admin-platform',
    captureId: 'admin-platform-backup-region-update-no-region-market',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-admin-platform-backup-region-update-no-region-market.mts',
    purpose:
      'backupRegionUpdate REGION_NOT_FOUND when the country exists but no active non-legacy region market covers it.',
    requiredAuthScopes: ['active Admin API token with Markets/admin platform access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}admin-platform-backup-region-update-no-region-market.json`,
      'config/parity-specs/admin-platform/admin-platform-backup-region-update-no-region-market.json',
      'config/parity-requests/admin-platform/admin-platform-backup-region-update-no-region-market.graphql',
    ],
    cleanupBehavior:
      'Temporarily removes AT from a multi-country active region market, records backupRegionUpdate REGION_NOT_FOUND, then restores AT to the market.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    captureId: 'admin-platform-backup-region-update-access-blocker',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-admin-platform-backup-region-update-access-blocker.mts',
    purpose:
      'backupRegionUpdate pre-resolve Markets access denial for a short-lived delegate token without Markets scopes.',
    requiredAuthScopes: [
      'active Admin API token that can create delegate tokens and has Markets/admin platform access',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}admin-platform-backup-region-update-access-blocker.json`,
      'config/parity-specs/admin-platform/admin-platform-backup-region-update-access-blocker.json',
    ],
    cleanupBehavior:
      'Creates one short-lived read_products delegate token, records the denied backupRegionUpdate response through that token, then destroys the delegate token with the parent credential.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'admin-platform',
    captureId: 'admin-platform-flow-generate-signature-required-args',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-admin-platform-flow-generate-signature-required-args-conformance.mts',
    purpose:
      'flowGenerateSignature missing and literal-null required-argument GraphQL coercion validation before resolver execution.',
    requiredAuthScopes: ['active Admin API token with Admin GraphQL schema access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}admin-platform-flow-generate-signature-required-args.json`,
      'config/parity-specs/admin-platform/admin-platform-flow-generate-signature-required-args.json',
      'config/parity-requests/admin-platform/admin-platform-flow-generate-signature-required-args-missing-both.graphql',
      'config/parity-requests/admin-platform/admin-platform-flow-generate-signature-required-args-missing-id.graphql',
      'config/parity-requests/admin-platform/admin-platform-flow-generate-signature-required-args-missing-payload.graphql',
      'config/parity-requests/admin-platform/admin-platform-flow-generate-signature-required-args-null-id.graphql',
      'config/parity-requests/admin-platform/admin-platform-flow-generate-signature-required-args-null-payload.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture; requests fail GraphQL coercion before the resolver and do not mutate store data.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-refund-attribution-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-refund-attribution-validation-conformance.mts',
    purpose:
      'refundCreate public-schema attribution field coercion and unchanged downstream order read after rejection.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}refund-create-attribution-validation.json`,
      'config/parity-specs/orders/refundCreate-attribution-validation.json',
      'config/parity-requests/orders/refundCreate-attribution-validation.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable paid order, captures pre-resolver coercion failures, records an unchanged downstream read, then attempts orderCancel cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-refund-transactions-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-refund-transactions-validation-conformance.mts',
    purpose:
      'refundCreate transaction kind, parent transaction, gateway mismatch, and matching-parent happy path validation.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}refund-create-transactions-validation.json`,
      'config/parity-specs/orders/refundCreate-transactions-validation.json',
      'config/parity-requests/orders/refundCreate-transactions-validation.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable paid orders, captures rejected refundCreate transaction kind and parent validation, captures current public gateway-normalization behavior, captures one accepted baseline, then attempts orderCancel cleanup.',
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
      'config/parity-requests/orders/refund-order-hydrate.graphql',
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
    captureId: 'order-mark-as-paid-snapshot-staging',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-mark-as-paid-snapshot-staging-conformance.ts',
    purpose:
      'orderMarkAsPaid snapshot staging without money-bag selection, read-after-write, already-paid validation, and unknown-id validation.',
    requiredAuthScopes: ['read_orders', 'write_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderMarkAsPaid-snapshot-staging.json`,
      `${CAPTURE_ROOT}orderMarkAsPaid-snapshot-staging-cleanup.json`,
      'config/parity-specs/orders/orderMarkAsPaid-snapshot-staging.json',
      'config/parity-requests/orders/orderMarkAsPaid-snapshot-staging-create.graphql',
      'config/parity-requests/orders/orderMarkAsPaid-snapshot-staging-mark.graphql',
      'config/parity-requests/orders/orderMarkAsPaid-snapshot-staging-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable markable and already-paid orders, marks the markable order paid, records validation branches, then attempts orderCancel cleanup.',
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
      'config/parity-requests/orders/return-reverse-logistics-read-recorded.graphql',
    ],
    cleanupBehavior:
      'Creates and fulfills a disposable order, records return/reverse-logistics lifecycle evidence, then cancels the order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'return-reverse-logistics-dispose-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-return-reverse-logistics-dispose-validation-conformance.mts',
    purpose:
      'reverseFulfillmentOrderDispose validation userErrors for empty inputs, custom-line RESTOCKED, multiple reverse fulfillment orders, current public unknown-line behavior, plus valid NOT_RESTOCKED downstream readback.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_returns', 'write_returns', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}return-reverse-logistics-dispose-validation.json`,
      'config/parity-specs/orders/return-reverse-logistics-dispose-validation.json',
      'config/parity-requests/orders/reverse-fulfillment-order-dispose-validation.graphql',
      'config/parity-requests/orders/return-reverse-logistics-dispose-validation-read.graphql',
    ],
    cleanupBehavior:
      'Creates and fulfills a disposable custom-line order, records invalid dispose attempts before one valid disposal, then attempts orderCancel cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'return-shipping-fee',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-return-shipping-fee-conformance.mts',
    purpose:
      'returnCreate ReturnInput.returnShippingFee and read-after-write Return.returnShippingFees behavior, plus public-schema evidence for hidden ReturnInput field boundaries.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_returns', 'write_returns', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}return-shipping-fee-recorded.json`,
      'config/parity-specs/orders/return-shipping-fee-recorded.json',
      'config/parity-requests/orders/return-create-shipping-fee-recorded.graphql',
      'config/parity-requests/orders/return-shipping-fee-read-recorded.graphql',
    ],
    cleanupBehavior:
      'Creates and fulfills a disposable order, records returnCreate shipping-fee evidence and downstream reads, then cancels and deletes the order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'return-customer-note',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-return-customer-note-conformance.mts',
    purpose:
      'returnRequest ReturnRequestLineItemInput.customerNote mutation payload echo and read-after-write ReturnLineItem.customerNote behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_returns', 'write_returns', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}return-customer-note-recorded.json`,
      'config/parity-specs/orders/return-request-customer-note-recorded.json',
      'config/parity-requests/orders/return-request-customer-note-recorded.graphql',
      'config/parity-requests/orders/return-customer-note-read-recorded.graphql',
    ],
    cleanupBehavior:
      'Creates and fulfills a disposable custom-line order, records returnRequest customerNote evidence and downstream return(id:) readback, then cancels and deletes the order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'return-reason-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-return-reason-validation-conformance.mts',
    purpose:
      'returnCreate and returnRequest return line item returnReason required, OTHER note, reason-definition OTHER note, and invalid enum boundary validation.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_returns', 'write_returns', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}return-reason-validation.json`,
      'config/parity-specs/orders/return-reason-validation.json',
      'config/parity-requests/orders/return-create-reason-validation.graphql',
      'config/parity-requests/orders/return-request-reason-validation.graphql',
    ],
    cleanupBehavior:
      'Creates and fulfills a disposable order, records invalid returnCreate/returnRequest attempts that should not stage returns, then cancels and deletes the order.',
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
      'config/parity-specs/orders/return-reverse-logistics-non-recording-operation-name.json',
      'config/parity-specs/orders/return-request-decline-local-staging.json',
      'config/parity-specs/orders/removeFromReturn-local-staging.json',
      'config/parity-specs/orders/returnApprove-decline-state-preconditions.json',
      'config/parity-requests/orders/return-reverse-non-recording-approve.graphql',
      'config/parity-requests/orders/return-reverse-non-recording-delivery-create.graphql',
      'config/parity-requests/orders/return-reverse-non-recording-delivery-update.graphql',
      'config/parity-requests/orders/return-reverse-non-recording-dispose.graphql',
      'config/parity-requests/orders/return-reverse-non-recording-process.graphql',
      'config/parity-requests/orders/return-reverse-non-recording-read.graphql',
      'config/parity-requests/orders/return-reverse-non-recording-request.graphql',
    ],
    cleanupBehavior: 'Read-only introspection; no cleanup expected.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'return-decline-request-validation',
    scriptPath: 'scripts/capture-return-decline-request-validation-conformance.ts',
    purpose:
      'Public Admin GraphQL returnDeclineRequest declineReason enum validation evidence and public-schema boundary evidence for tmp_notify_customer notification payloads.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_returns', 'write_returns'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}return-decline-request-validation.json`,
      'config/parity-specs/orders/return-request-decline-local-staging.json',
    ],
    cleanupBehavior:
      'Validation-only capture uses unknown Return/Order IDs and GraphQL variable coercion failures; it creates no live Shopify objects.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The public Admin schema rejects tmp_notify_customer as an unknown field on returnDeclineRequest, returnApproveRequest, and returnRequest. Hidden-payload email validation remains executable local-runtime evidence.',
  },
  {
    domain: 'orders',
    captureId: 'return-approve-decline-state-preconditions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-return-approve-decline-state-preconditions-conformance.mts',
    purpose:
      'returnApproveRequest and returnDeclineRequest invalid-state userError shapes for OPEN and DECLINED returns, already-declined decline, and unknown Return IDs.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_returns', 'write_returns', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}returnApprove-decline-state-preconditions.json`,
      'config/parity-specs/orders/returnApprove-decline-state-preconditions-live.json',
      'config/parity-requests/orders/return-approve-request-recorded.graphql',
      'config/parity-requests/orders/return-decline-request-local-staging.graphql',
      'config/parity-requests/orders/return-order-hydrate.graphql',
    ],
    cleanupBehavior:
      'Creates and fulfills disposable orders, transitions returns to OPEN/DECLINED, records rejected approve/decline branches, then cancels the orders.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'orders',
    captureId: 'order-update-snapshot-staging-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-order-update-snapshot-staging-local-runtime.ts',
    purpose:
      'Local-runtime recording for orderCreate-backed orderUpdate happy-path staging, downstream order reads, and raw mutation-log retention.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}orderUpdate-snapshot-staging.json`,
      `${LOCAL_RUNTIME_ROOT}orderUpdate-snapshot-staging.json`,
      'config/parity-specs/orders/orderUpdate-snapshot-staging.json',
      'config/parity-requests/orders/orderUpdate-snapshot-staging-create.graphql',
      'config/parity-requests/orders/orderUpdate-snapshot-staging-create.variables.json',
      'config/parity-requests/orders/orderUpdate-snapshot-staging.graphql',
      'config/parity-requests/orders/orderUpdate-snapshot-staging-read.graphql',
    ],
    cleanupBehavior:
      'Runs only against the local Rust proxy runtime through public GraphQL requests; no Shopify cleanup required.',
    expectedStatusChecks: ['conformance:check', 'rust:test', 'targeted-runtime-test'],
  },
  {
    domain: 'orders',
    captureId: 'return-decline-request-local-runtime',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-return-decline-request-local-runtime.ts',
    purpose:
      'Local-runtime recording for returnDeclineRequest valid decline, invalid declineReason, and hidden tmp_notify_customer email validation branches.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}return-quantity-validation.json`,
      `${LOCAL_RUNTIME_ROOT}return-lifecycle-local-staging.json`,
      'config/parity-specs/orders/return-request-decline-local-staging.json',
    ],
    cleanupBehavior:
      'Runs only against the local proxy runtime with a deterministic order-hydration cassette; no Shopify cleanup required.',
    expectedStatusChecks: ['conformance:check', 'rust:test', 'targeted-runtime-test'],
  },
  {
    domain: 'orders',
    captureId: 'return-status-preconditions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-return-status-preconditions-conformance.mts',
    purpose:
      'returnClose, returnReopen, returnCancel, and removeFromReturn status-machine/editability preconditions, idempotent no-op branches, and processed-return cancel rejection.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_returns', 'write_returns', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}returnClose-Reopen-Cancel-state-preconditions.json`,
      'config/parity-specs/orders/returnClose-Reopen-Cancel-state-preconditions.json',
      'config/parity-requests/orders/return-cancel-state-precondition.graphql',
      'config/parity-requests/orders/return-close-state-precondition.graphql',
      'config/parity-requests/orders/remove-from-return-state-precondition.graphql',
      'config/parity-requests/orders/return-reopen-state-precondition.graphql',
      'config/parity-requests/orders/return-order-hydrate.graphql',
      'config/parity-requests/orders/return-remove-from-return-state-precondition-read.graphql',
    ],
    cleanupBehavior:
      'Creates and fulfills disposable orders for requested, open/closed, cancelable, declined, and processed return states, records status/editability precondition behavior, then cancels the orders.',
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
    domain: 'store-properties',
    captureId: 'location-activate-non-unique-name',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-location-activate-non-unique-name-conformance.mts',
    purpose:
      'locationActivate HAS_NON_UNIQUE_NAME validation when an inactive target name collides with another active shop location.',
    requiredAuthScopes: ['read_locations', 'write_locations'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/store-properties/location-activate-non-unique-name.json',
      'config/parity-specs/store-properties/location-activate-non-unique-name.json',
      'config/parity-requests/store-properties/location-activate-non-unique-observed-active.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable locations with the same name by deactivating the target before creating the active duplicate, records the rejected activation, then deactivates/deletes both locations.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-order-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-order-lifecycle-conformance.ts',
    purpose: 'Fulfillment order hold/request/cancel/close lifecycle behavior.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-order-lifecycle.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-order-lifecycle-local-staging.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-lifecycle-order-read.graphql',
    ],
    cleanupBehavior: 'Cancels disposable order and records cleanup captures.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-order-open-report-progress-preconditions',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-order-open-report-progress-preconditions-conformance.ts',
    purpose:
      'fulfillmentOrderOpen and fulfillmentOrderReportProgress invalid-state public userError field/message behavior plus unchanged downstream reads.',
    requiredAuthScopes: [
      'read_orders',
      'write_orders',
      'read_fulfillments',
      'write_fulfillments',
      'write_merchant_managed_fulfillment_orders',
      'write_assigned_fulfillment_orders',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-order-open-report-progress-preconditions.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-order-open-report-progress-preconditions.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-status-precondition-open.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-status-precondition-report-progress.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-status-precondition-order-read.graphql',
    ],
    cleanupBehavior:
      'Discovers an existing closed fulfillment order, creates one disposable held order, captures rejected mutations, then cancels the disposable order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Public Admin GraphQL 2026-04 exposes field/message for these userErrors; local runtime tests cover proxy-only code projection and the store-state CANCELLED branch.',
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-order-close-state',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-order-close-state-conformance.ts',
    purpose:
      'fulfillmentOrderClose success state after a fulfillment order is moved to an API fulfillment service, submitted, and accepted.',
    requiredAuthScopes: [
      'read_orders',
      'write_orders',
      'read_fulfillments',
      'write_fulfillments',
      'fulfillment service management',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-order-close-state.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-order-close-state.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-close-state-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable fulfillment service and one disposable order, closes the accepted fulfillment order, then cancels the order and deletes the service.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The captured public Admin API behavior transitions close success to status INCOMPLETE with requestStatus CLOSED.',
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-order-release-hold-selective',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-order-release-hold-selective-conformance.ts',
    purpose: 'fulfillmentOrderReleaseHold holdIds selective release behavior and remaining-hold ON_HOLD status.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-order-release-hold-selective.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-order-release-hold-selective.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-release-hold-selective-hold.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-release-hold-selective-release.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-release-hold-selective-order-read.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-release-hold-selective-hydrate.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable order, places two requesting-app holds, records selective release, releases the remaining hold, then cancels the order.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Cross-app hold ownership rejection remains runtime-test-backed because the current conformance target has one app credential.',
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
    captureId: 'fulfillment-order-move-hold-multi-line',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-order-move-hold-multi-line-conformance.ts',
    purpose: 'fulfillmentOrderMove and fulfillmentOrderHold multi-line-item partial quantity handling.',
    requiredAuthScopes: ['read_orders', 'write_orders', 'read_fulfillments', 'write_fulfillments'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-order-move-multi-line.json`,
      `${CAPTURE_ROOT}fulfillment-order-hold-multi-line.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-order-move-multi-line.json',
      'config/parity-specs/shipping-fulfillments/fulfillment-order-hold-multi-line.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-move-hold-multi-line-locations.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-move-multi-line.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-order-hold-multi-line.graphql',
    ],
    cleanupBehavior:
      'Creates disposable multi-line orders, captures partial move and hold branches, releases created holds, then cancels the orders.',
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
    captureId: 'fulfillment-service-uniqueness',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-service-uniqueness-conformance.ts',
    purpose:
      'fulfillmentServiceCreate and fulfillmentServiceUpdate per-shop name and generated-handle uniqueness validation.',
    requiredAuthScopes: ['read_fulfillments', 'write_fulfillments', 'read_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-service-uniqueness.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-service-uniqueness.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-uniqueness-create.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-uniqueness-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable fulfillment services for uniqueness probes, records rejected duplicate create/update branches, then deletes all created services.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-service-name-whitespace-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-service-name-whitespace-validation-conformance.ts',
    purpose: 'fulfillmentServiceCreate and fulfillmentServiceUpdate name leading/trailing whitespace validation.',
    requiredAuthScopes: ['read_fulfillments', 'write_fulfillments', 'read_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-service-name-whitespace-validation.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-service-name-whitespace-validation.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-name-whitespace-primary.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-name-whitespace-update.graphql',
    ],
    cleanupBehavior:
      'Records one validation-only create, creates one disposable fulfillment service for update validation, records rejected update, then deletes the created service.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-service-requires-shipping-method-default',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-service-requires-shipping-method-default-conformance.ts',
    purpose:
      'fulfillmentServiceCreate and fulfillmentServiceUpdate requiresShippingMethod default_value behavior when the argument is omitted.',
    requiredAuthScopes: ['read_fulfillments', 'write_fulfillments', 'read_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-service-requires-shipping-method-default.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-service-requires-shipping-method-default.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-requires-shipping-default-create-omitted.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-requires-shipping-default-create-false.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-requires-shipping-default-update-omitted.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-requires-shipping-default-read.graphql',
    ],
    cleanupBehavior:
      'Creates two disposable fulfillment services, records omitted create/update read-after-write behavior, then deletes both services.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-service-permits-sku-sharing-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-service-permits-sku-sharing-validation-conformance.ts',
    purpose:
      'fulfillmentServiceCreate removed permitsSkuSharing argument validation plus inventoryManagement create/update downstream read parity.',
    requiredAuthScopes: ['read_fulfillments', 'write_fulfillments', 'read_locations'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-service-permits-sku-sharing-validation.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-service-permits-sku-sharing-validation.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-permits-sku-sharing-validation.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-inventory-management-create.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-inventory-management-read.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-inventory-management-update.graphql',
    ],
    cleanupBehavior:
      'Captures schema validation for the removed argument without creating a record, then creates one disposable fulfillment service for inventoryManagement read-after-write evidence and deletes it.',
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
    captureId: 'delivery-profile-create-disallowed-update-keys',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-delivery-profile-create-disallowed-update-keys-conformance.ts',
    purpose:
      'deliveryProfileCreate validation for create-time update-only keys and allowed method-definition create input.',
    requiredAuthScopes: ['read_shipping', 'write_shipping', 'delivery profile management access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}delivery-profile-create-disallowed-update-keys.json`,
      'config/parity-specs/shipping-fulfillments/delivery-profile-create-disallowed-update-keys.json',
      'config/parity-requests/shipping-fulfillments/delivery-profile-create-disallowed-update-keys.graphql',
    ],
    cleanupBehavior:
      'Records validation-only rejection branches, then creates one disposable allowed profile and removes it in cleanup.',
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
    captureId: 'carrier-service-update-blank-name',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-carrier-service-update-blank-name-conformance.ts',
    purpose:
      'DeliveryCarrierService update validation for present blank name, including typed CARRIER_SERVICE_UPDATE_FAILED userError and unchanged downstream read state.',
    requiredAuthScopes: ['read_shipping', 'write_shipping'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}carrier-service-update-blank-name.json`,
      'config/parity-specs/shipping-fulfillments/carrier-service-update-blank-name.json',
      'config/parity-requests/shipping-fulfillments/carrier-service-update-blank-name-create.graphql',
      'config/parity-requests/shipping-fulfillments/carrier-service-update-blank-name-read.graphql',
      'config/parity-requests/shipping-fulfillments/carrier-service-update-blank-name.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable carrier service, records rejected blank-name update and read-after-reject state, then deletes the carrier service in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'carrier-service-create-uniqueness',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-carrier-service-create-uniqueness-conformance.ts',
    purpose:
      'DeliveryCarrierService active per-app uniqueness validation for carrierServiceCreate duplicate active services.',
    requiredAuthScopes: ['read_shipping', 'write_shipping'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}carrier-service-create-uniqueness.json`,
      'config/parity-specs/shipping-fulfillments/carrier-service-create-uniqueness.json',
      'config/parity-requests/shipping-fulfillments/carrier-service-create-uniqueness.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable active carrier service, records the duplicate create rejection for the same app/shop, then deletes the created carrier service.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'carrier-service-create-required-fields',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-carrier-service-create-required-fields-conformance.ts',
    purpose:
      'DeliveryCarrierServiceCreateInput active/supportsServiceDiscovery required-field coercion validation for carrierServiceCreate.',
    requiredAuthScopes: ['read_shipping', 'write_shipping'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}carrier-service-create-required-fields.json`,
      'config/parity-specs/shipping-fulfillments/carrier-service-create-required-fields.json',
      'config/parity-requests/shipping-fulfillments/carrier-service-create-required-fields.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture; omitted required input fields fail GraphQL variable coercion before carrierServiceCreate can create live objects.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-service-callback-url-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-service-callback-url-validation-conformance.ts',
    purpose: 'Current app-scoped FulfillmentService callbackUrl allow/deny behavior for create and update.',
    requiredAuthScopes: ['read_assigned_fulfillment_orders', 'write_assigned_fulfillment_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-service-callback-url-validation.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-service-callback-url-validation.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-callback-url-validation-update-allowed.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-callback-url-validation-update-disallowed.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-callback-url-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable fulfillment services with allowed callback URLs, records invalid create/update attempts, then deletes the created fulfillment services in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'fulfillment-service-callback-url-update-protocol-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-fulfillment-service-callback-url-update-protocol-conformance.ts',
    purpose: 'FulfillmentService callbackUrl protocol validation for update.',
    requiredAuthScopes: ['read_assigned_fulfillment_orders', 'write_assigned_fulfillment_orders'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}fulfillment-service-callback-url-update-protocol-validation.json`,
      'config/parity-specs/shipping-fulfillments/fulfillment-service-callback-url-update-protocol-validation.json',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-callback-url-update-protocol-create.graphql',
      'config/parity-requests/shipping-fulfillments/fulfillment-service-callback-url-validation-update-protocol.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable fulfillment service with an allowed callback URL, records an invalid ftp:// callbackUrl update attempt, then deletes the fulfillment service in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'delivery-profiles',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
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
    captureId: 'delivery-profile-update-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-delivery-profile-update-validation-conformance.ts',
    purpose:
      'deliveryProfileUpdate validation behavior for oversized names, unknown location references, empty zone countries, and public update probes.',
    requiredAuthScopes: ['read_shipping', 'write_shipping', 'delivery profile management access'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/delivery-profile-update-validation.json',
      'config/parity-specs/shipping-fulfillments/delivery-profile-update-validation.json',
      'config/parity-requests/shipping-fulfillments/delivery-profile-update-validation-create.graphql',
      'config/parity-requests/shipping-fulfillments/delivery-profile-update-validation.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable delivery profile, records invalid update attempts and public update probes against it, then removes the profile in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'delivery-profile-name-boundary',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-delivery-profile-name-boundary-conformance.ts',
    purpose: 'deliveryProfileCreate and deliveryProfileUpdate name length boundary behavior at 128 and 129 characters.',
    requiredAuthScopes: ['read_shipping', 'write_shipping', 'delivery profile management access'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/shipping-fulfillments/delivery-profile-name-boundary.json',
      'config/parity-specs/shipping-fulfillments/delivery-profile-name-boundary.json',
    ],
    cleanupBehavior:
      'Creates one disposable delivery profile with a 128-character name, records 128-character update and 129-character create/update boundaries, then removes the profile in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'delivery-profile-default-update',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-delivery-profile-default-update-conformance.ts',
    purpose: 'Name-only deliveryProfileUpdate behavior for the shop default delivery profile.',
    requiredAuthScopes: ['read_shipping', 'write_shipping', 'delivery profile management access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}delivery-profile-default-update.json`,
      'config/parity-specs/shipping-fulfillments/delivery-profile-default-update.json',
      'config/parity-requests/shipping-fulfillments/delivery-profile-default-update.graphql',
    ],
    cleanupBehavior:
      'Finds the existing default delivery profile, updates its name for the capture, reads it back, then restores the original name in cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'shipping-fulfillments',
    captureId: 'shipping-settings',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
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
      'config/parity-specs/bulk-operations/bulk-operation-cancel-status-branches.json',
      'config/parity-specs/bulk-operations/bulk-operation-run-query-created-status.json',
      'config/parity-specs/bulk-operations/bulk-operation-status-catalog-cancel.json',
      'config/parity-specs/bulk-operations/bulk-operation-read-after-write-consumer-poll.json',
      'config/parity-requests/bulk-operations/bulk-operation-consumer-poll.graphql',
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
    captureId: 'bulk-operations-sort-key',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operations-sort-key-conformance.ts',
    purpose:
      'bulkOperations public CREATED_AT/COMPLETED_AT ordering in both directions and public-schema rejection for hidden ID sort key.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operations-sort-key.json`,
      'config/parity-specs/bulk-operations/bulk-operations-sort-key.json',
      'config/parity-requests/bulk-operations/bulk-operations-sort-key.graphql',
      'config/parity-requests/bulk-operations/bulk-operations-sort-key-id-rejected.graphql',
    ],
    cleanupBehavior: 'Read-only capture; no Shopify data is created or mutated.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'bulk-query-schema',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-query-schema.mts',
    purpose: 'Admin GraphQL output-field connection/list/object schema facts used by bulkOperationRunQuery validation.',
    requiredAuthScopes: ['schema introspection access through the active Admin token'],
    fixtureOutputs: [
      'config/admin-graphql-bulk-query-schema.json',
      'src/shopify_draft_proxy/proxy/bulk_query_schema_data.gleam',
    ],
    cleanupBehavior: 'Read-only introspection; no cleanup expected.',
    expectedStatusChecks: ['conformance:check', 'conformance:status'],
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
    captureId: 'bulk-operation-cancel-preserves-fields',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-cancel-preserves-fields-conformance.ts',
    purpose:
      'bulkOperationCancel preserves non-status fields when canceling a non-terminal operation with accumulated counters.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-cancel-preserves-fields.json`,
      'config/parity-specs/bulk-operations/bulk-operation-cancel-preserves-fields.json',
    ],
    cleanupBehavior:
      'Starts a safe product bulk query and cancels it after Shopify reports non-zero progress counters.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'bulk-operation-run-query-schema-roots',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-run-query-schema-roots-conformance.ts',
    purpose:
      'bulkOperationRunQuery success evidence for schema-known non-product roots and list-but-not-connection fields outside the former curated list.',
    requiredAuthScopes: ['read_orders', 'read_draft_orders'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-query-schema-roots.json',
      'config/parity-specs/bulk-operations/bulk-operation-run-query-schema-roots.json',
      'config/parity-requests/bulk-operations/bulk-operation-run-query-schema-roots.graphql',
    ],
    cleanupBehavior:
      'Starts short-lived query bulk operations and cancels any active query bulk operation before/after each captured case.',
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
    captureId: 'bulk-operation-storage-byte-limit',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-storage-byte-limit-conformance.ts',
    purpose:
      'bulkOperationRunQuery and bulkOperationRunMutation storage byte-limit validation for exactly 65,536 escaped UTF-8 bytes.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-storage-byte-limit.json`,
      'config/parity-specs/bulk-operations/bulk-operation-storage-byte-limit.json',
      'config/parity-requests/bulk-operations/bulk-operation-storage-byte-limit-query.graphql',
      'config/parity-requests/bulk-operations/bulk-operation-storage-byte-limit-mutation.graphql',
    ],
    cleanupBehavior:
      'Validation-only capture. It creates a staged upload target and uploads a tiny JSONL file for the mutation argument, but oversized validation prevents bulk job creation and catalog mutation.',
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
    captureId: 'bulk-operation-name-independent-run-roots',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-name-independence-conformance.ts',
    purpose:
      'bulkOperationRunQuery and bulkOperationRunMutation validation behavior is independent of client GraphQL operation names.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-name-independent-run-roots.json`,
      'config/parity-specs/bulk-operations/bulk-operation-name-independent-run-roots.json',
      'config/parity-requests/bulk-operations/bulk-operation-name-independent-run-query.graphql',
      'config/parity-requests/bulk-operations/bulk-operation-name-independent-run-mutation.graphql',
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
    captureId: 'bulk-operation-client-identifier-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-client-identifier-conformance.ts',
    purpose:
      'bulkOperationRunMutation clientIdentifier length validation after successful staged-upload byte handoff, plus public-schema observation for bulkOperationRunQuery clientIdentifier.',
    requiredAuthScopes: ['bulk operation access and product write access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-run-mutation-client-identifier-validation.json`,
      'config/parity-specs/bulk-operations/bulk-operation-run-mutation-client-identifier-validation.json',
      'config/parity-requests/bulk-operations/bulk-operation-client-identifier-staged-upload.graphql',
      'config/parity-requests/bulk-operations/bulk-operation-run-mutation-client-identifier-validation.graphql',
    ],
    cleanupBehavior:
      'Cancels any pre-existing mutation bulk operation, uploads JSONL bytes, then records validation-only branches rejected before product creation. POS/product-feed allowlist throttle scoping requires a POS-class credential and is recorded as unavailable when absent.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'bulk-operation-run-mutation-file-size',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-run-mutation-file-size-conformance.ts',
    purpose:
      'bulkOperationRunMutation oversized staged JSONL validation returns INVALID_STAGED_UPLOAD_FILE before in-progress mutation throttle handling.',
    requiredAuthScopes: ['bulk operation access and product write access through active Admin token'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-run-mutation-file-size.json`,
      'config/parity-specs/bulk-operations/bulk-operation-run-mutation-file-size.json',
      'config/parity-requests/bulk-operations/bulk-operation-run-mutation-file-size-staged-upload.graphql',
      'config/parity-requests/bulk-operations/bulk-operation-run-mutation-file-size.graphql',
    ],
    cleanupBehavior:
      'Cancels any pre-existing mutation bulk operation, uploads one under-limit JSONL to create a non-terminal mutation operation, uploads one >100 MB JSONL, records the oversized validation response, then cancels the non-terminal operation.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'bulk-operation-run-mutation-allowed-roots',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-bulk-operation-run-mutation-allowed-roots-conformance.ts',
    purpose:
      'bulkOperationRunMutation accepts non-products inner mutation roots and returns CREATED for customerCreate, metaobjectDefinitionCreate, and metafieldsSet bulk imports.',
    requiredAuthScopes: [
      'bulk operation access through active Admin token',
      'write_customers',
      'write_metaobjects',
      'write_metafields',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}bulk-operation-run-mutation-allowed-roots.json`,
      'config/parity-specs/bulk-operations/run-mutation-allowed-roots.json',
      'config/parity-requests/bulk-operations/bulk-operation-run-mutation-allowed-roots.graphql',
    ],
    cleanupBehavior:
      'Uploads three JSONL files, submits customerCreate, metaobjectDefinitionCreate, and metafieldsSet bulk mutations, waits for terminal completion, then deletes the created customer, metaobject definition, and shop metafield.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'bulk-operations',
    captureId: 'platform-payments-orphaned-fixtures-bulk-operations',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04', ORPHAN_FIXTURE_GROUP: 'bulk-operations' },
    scriptPath: 'scripts/capture-platform-payments-orphaned-fixtures-conformance.ts',
    purpose: 'Re-records bulkOperationRunMutation validator fixtures consumed by the standard parity runner.',
    requiredAuthScopes: ['bulk operation access through active Admin token'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-mutation-validators.json',
      'config/parity-specs/bulk-operations/bulk-operation-run-mutation-validators.json',
      'config/parity-requests/bulk-operations/bulk-operation-run-mutation-validators.graphql',
    ],
    cleanupBehavior: 'Validation-only capture; no Shopify data is created or mutated.',
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
    domain: 'bulk-operations',
    captureId: 'bulk-operation-concurrency-limit',
    environment: {
      SHOPIFY_CONFORMANCE_BULK_API_VERSION: '2026-04',
      SHOPIFY_CONFORMANCE_BULK_NEW_LIMIT_BOUNDARY: '1',
    },
    scriptPath: 'scripts/capture-bulk-operation-in-progress-conformance.ts',
    purpose:
      'bulkOperationRunQuery and bulkOperationRunMutation 2026-04 concurrency limit boundaries: five same-type non-terminal operations accepted, sixth throttled.',
    requiredAuthScopes: ['bulk operation access through active Admin token', 'write_products'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-query-concurrency-limit.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/bulk-operations/bulk-operation-run-mutation-concurrency-limit.json',
      'config/parity-specs/bulk-operations/bulk-operation-run-query-concurrency-limit.json',
      'config/parity-specs/bulk-operations/bulk-operation-run-mutation-concurrency-limit.json',
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
    purpose: 'Webhook subscription create/read/update/delete, filter validation, and access-scope observations.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-cloud-uri-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-conformance.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-topic-enum-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-topic-format-name-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-uri-validation.json',
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/webhooks/webhook-subscription-uri-whitespace.json',
      'config/parity-specs/webhooks/webhook-subscription-payload-fields.json',
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
    captureId: 'webhook-subscription-dedicated-cloud-destinations',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-dedicated-cloud-destinations-conformance.ts',
    purpose:
      'Dedicated Pub/Sub and EventBridge webhook subscription create/update lifecycle, downstream reads, and typed-input validation branches.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-dedicated-cloud-destinations.json`,
      'config/parity-specs/webhooks/webhook-subscription-dedicated-cloud-destinations.json',
      'config/parity-requests/webhooks/pubSubWebhookSubscriptionCreate-parity.graphql',
      'config/parity-requests/webhooks/pubSubWebhookSubscriptionUpdate-parity.graphql',
      'config/parity-requests/webhooks/eventBridgeWebhookSubscriptionCreate-parity.graphql',
      'config/parity-requests/webhooks/eventBridgeWebhookSubscriptionUpdate-parity.graphql',
      'config/parity-requests/webhooks/webhook-subscription-dedicated-cloud-detail-read.graphql',
    ],
    cleanupBehavior:
      'Creates temporary Pub/Sub and EventBridge SHOP_UPDATE subscriptions, records validation branches, then deletes the temporary subscriptions during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Records deprecated dedicated roots that synthesize cloud destination addresses before delegating to the shared webhook subscription resolver path.',
  },
  {
    domain: 'webhooks',
    captureId: 'eventbridge-cloud-format-json-only',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-eventbridge-cloud-format-json-only-conformance.ts',
    purpose:
      'EventBridge ARN cloud-delivery JSON-only format validation for dedicated EventBridge and unified webhook subscription create/update roots.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}eventbridge-cloud-format-json-only.json`,
      'config/parity-specs/webhooks/eventbridge-cloud-format-json-only.json',
    ],
    cleanupBehavior:
      'Creates two temporary JSON EventBridge webhook subscriptions as update targets, records ARN + XML validation failures, then deletes the temporary subscriptions during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Confirms the dedicated EventBridge roots keep Shopify’s model-level format userError field path as ["webhookSubscription", "format"].',
  },
  {
    domain: 'webhooks',
    captureId: 'gcp-project-topic-char-rules',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-gcp-project-topic-char-rules-conformance.ts',
    purpose:
      'GCP Pub/Sub project/topic character validation for dedicated Pub/Sub and unified webhook subscription create/update roots.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gcp-project-topic-char-rules.json`,
      'config/parity-specs/webhooks/gcp-project-topic-char-rules.json',
    ],
    cleanupBehavior:
      'Creates temporary Pub/Sub webhook subscriptions for accepted create/update branches, records validation failures, then deletes temporary subscriptions during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Records numeric project-number acceptance, digit-leading topic rejection, and percent-topic acceptance for both dedicated and unified webhook roots.',
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscription-pub-sub-required-fields',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-pub-sub-required-fields-conformance.ts',
    purpose:
      'Pub/Sub dedicated webhook subscription create/update required pubSubProject/pubSubTopic input-field validation.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-pub-sub-required-fields.json`,
      'config/parity-specs/webhooks/webhook-subscription-pub-sub-required-fields.json',
      'config/parity-requests/webhooks/pubSubWebhookSubscriptionCreate-missing-project.graphql',
      'config/parity-requests/webhooks/pubSubWebhookSubscriptionCreate-missing-topic.graphql',
      'config/parity-requests/webhooks/pubSubWebhookSubscriptionCreate-missing-project-topic.graphql',
      'config/parity-requests/webhooks/pubSubWebhookSubscriptionUpdate-missing-project.graphql',
      'config/parity-requests/webhooks/pubSubWebhookSubscriptionUpdate-missing-topic.graphql',
      'config/parity-requests/webhooks/pubSubWebhookSubscriptionUpdate-missing-project-topic.graphql',
    ],
    cleanupBehavior:
      'All captured branches fail GraphQL variable validation before resolver execution; no webhook subscriptions are created, updated, deleted, or delivered.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Records top-level INVALID_VARIABLE errors for missing Pub/Sub project/topic fields on deprecated dedicated Pub/Sub roots.',
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscription-metafield-namespaces-resolution',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-subscription-metafield-namespaces-resolution.ts',
    purpose:
      'Webhook subscription metafieldNamespaces `$app:` resolution for create/update plus downstream detail/list reads.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-metafield-namespaces-resolution.json`,
      'config/parity-specs/webhooks/webhook-subscription-metafield-namespaces-resolution.json',
      'config/parity-requests/webhooks/webhook-subscription-metafield-namespaces-list.graphql',
    ],
    cleanupBehavior:
      'Creates one temporary PRODUCTS_UPDATE webhook subscription, updates it, captures downstream reads, and deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscription-metafields-lifecycle',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-subscription-metafields-conformance.ts',
    purpose:
      'WebhookSubscription.metafields input/output lifecycle for create/update plus downstream detail/list reads and omitted-input empty-list behavior.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-metafields-lifecycle.json`,
      'config/parity-specs/webhooks/webhook-subscription-metafields-lifecycle.json',
      'config/parity-requests/webhooks/webhook-subscription-metafields-create.graphql',
      'config/parity-requests/webhooks/webhook-subscription-metafields-update.graphql',
      'config/parity-requests/webhooks/webhook-subscription-metafields-detail-read.graphql',
      'config/parity-requests/webhooks/webhook-subscription-metafields-list.graphql',
    ],
    cleanupBehavior:
      'Creates temporary SHOP_UPDATE webhook subscriptions for supplied and omitted metafields branches, captures downstream reads, then deletes both subscriptions during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscription-api-version-projection',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-subscription-api-version-projection.ts',
    purpose:
      'WebhookSubscription.apiVersion projection for create/update payloads plus downstream detail and connection-node reads.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-api-version-projection.json`,
      'config/parity-specs/webhooks/webhook-subscription-api-version-projection.json',
      'config/parity-requests/webhooks/webhook-subscription-api-version-create.graphql',
      'config/parity-requests/webhooks/webhook-subscription-api-version-update.graphql',
      'config/parity-requests/webhooks/webhook-subscription-api-version-detail-read.graphql',
      'config/parity-requests/webhooks/webhook-subscription-api-version-list.graphql',
    ],
    cleanupBehavior:
      'Creates one temporary SHOP_UPDATE webhook subscription, updates it, captures downstream reads, and deletes it during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscription-address-byte-size-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-subscription-address-byte-size-validation.ts',
    purpose: "Webhook subscription address byte-size validation at and above Shopify's text-column limit.",
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-address-byte-size-validation.json`,
      'config/parity-specs/webhooks/webhook-subscription-address-byte-size-validation.json',
    ],
    cleanupBehavior:
      'Creates one temporary SHOP_UPDATE webhook subscription at the accepted byte-size boundary and deletes it during cleanup; the above-limit branch is validation-only.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscription-filter-byte-size-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-subscription-filter-byte-size-validation.ts',
    purpose:
      'WebhookSubscriptionInput.filter byte-size validation for accepted-at-limit create, oversized create, and oversized update.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-filter-byte-size-validation.json`,
      'config/parity-specs/webhooks/webhook-subscription-filter-byte-size-validation.json',
    ],
    cleanupBehavior: 'Deletes accepted-at-limit and update-base webhook subscriptions during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'webhooks',
    captureId: 'webhook-subscription-topic-format-name-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-webhook-subscription-topic-format-name-validation.ts',
    purpose:
      'Webhook subscription topic/format, cloud format, name, duplicate active registration userErrors, and same-endpoint different-format acceptance.',
    requiredAuthScopes: ['webhook subscription management access for the installed app'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}webhook-subscription-topic-format-name-validation.json`,
      'config/parity-specs/webhooks/webhook-subscription-topic-format-name-validation.json',
    ],
    cleanupBehavior:
      'Creates temporary SHOP_UPDATE and PRODUCTS_UPDATE webhook subscriptions and deletes them during cleanup.',
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
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-gift-card-create-validation-conformance.ts',
    purpose:
      'Gift-card create validation for initial value, code length/format/uniqueness, missing customer, combined invalid-code plus missing-customer precedence, and generated code behavior.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-create-validation.json`,
      'config/parity-specs/gift-cards/gift-card-create-validation.json',
      'config/parity-specs/gift-cards/gift-card-ordinary-operation-names.json',
      'config/parity-requests/gift-cards/gift-card-ordinary-operation-names.graphql',
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
    captureId: 'gift-card-credit-limit-exceeded',
    scriptPath: 'scripts/capture-gift-card-credit-limit-exceeded-conformance.ts',
    purpose:
      'Gift-card credit validation when a card at the configured issue limit is credited past that limit, plus the debit-after-rejection public Admin path.',
    requiredAuthScopes: [
      'read_gift_cards',
      'write_gift_cards',
      'read_gift_card_transactions',
      'write_gift_card_transactions',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-credit-limit-exceeded.json`,
      'config/parity-specs/gift-cards/gift-card-credit-limit-exceeded.json',
      'config/parity-requests/gift-cards/gift-card-credit-limit-exceeded.graphql',
    ],
    cleanupBehavior:
      'Reads giftCardConfiguration.issueLimit, creates one boundary gift card, records an over-limit credit rejection and a one-cent debit probe, then deactivates the setup gift card.',
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
    captureId: 'gift-card-recipient-validation',
    scriptPath: 'scripts/capture-gift-card-recipient-validation-conformance.ts',
    purpose:
      'Gift-card create/update recipientAttributes validation for required recipient id, nonexistent recipient id, blank text fields, text length caps, HTML-tag rejection, and sendNotificationAt date range bounds.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-recipient-validation.json`,
      'config/parity-specs/gift-cards/gift-card-recipient-validation.json',
      'config/parity-requests/gift-cards/gift-card-recipient-validation.graphql',
      'config/parity-requests/gift-cards/gift-card-recipient-validation-create-missing-id.graphql',
      'config/parity-requests/gift-cards/gift-card-recipient-validation-update-missing-id.graphql',
      'config/parity-requests/gift-cards/gift-card-recipient-validation-customer-create.graphql',
      'config/parity-requests/gift-cards/gift-card-recipient-validation-gift-card-create.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable customer plus one active gift card, records recipient validation branches, deactivates setup gift cards, and deletes setup customers.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'The public giftCardUpdate userErrors type in Admin API 2025-01 exposes field/message only, so replay expectations add the typed code contract used by the local model.',
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
    captureId: 'gift-card-update-clear-nullable',
    scriptPath: 'scripts/capture-gift-card-update-clear-nullable-conformance.ts',
    purpose:
      'Gift-card update behavior for explicit null note, expiresOn, and templateSuffix fields against a populated gift card, including downstream readback.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-update-clear-nullable.json`,
      'config/parity-specs/gift-cards/gift-card-update-clear-nullable.json',
      'config/parity-requests/gift-cards/gift-card-update-clear-nullable.graphql',
      'config/parity-requests/gift-cards/gift-card-update-clear-nullable-create.graphql',
      'config/parity-requests/gift-cards/gift-card-update-clear-nullable-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable gift card with known nullable fields, records explicit-null clear branches and readback, and deactivates the setup gift card.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-create-omitted-optional-fields',
    scriptPath: 'scripts/capture-gift-card-create-omitted-optional-fields-conformance.ts',
    purpose:
      'Gift-card create behavior when note, expiresOn, customerId, templateSuffix, and recipientAttributes are omitted, including downstream readback.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-create-omitted-optional-fields.json`,
      'config/parity-specs/gift-cards/gift-card-create-omitted-optional-fields.json',
      'config/parity-requests/gift-cards/gift-card-create-omitted-optional-fields.graphql',
      'config/parity-requests/gift-cards/gift-card-create-omitted-optional-fields-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable gift card with only initialValue, records create payload and giftCard(id:) readback, and deactivates the setup gift card.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-template-suffix-prefix',
    scriptPath: 'scripts/capture-gift-card-template-suffix-prefix-conformance.ts',
    purpose:
      'Gift-card create/update behavior for templateSuffix values with the literal gift_card. prefix, including downstream readback.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-template-suffix-prefix.json`,
      'config/parity-specs/gift-cards/gift-card-template-suffix-prefix.json',
      'config/parity-requests/gift-cards/gift-card-template-suffix-prefix-create.graphql',
      'config/parity-requests/gift-cards/gift-card-template-suffix-prefix-update.graphql',
      'config/parity-requests/gift-cards/gift-card-template-suffix-prefix-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable gift card with a run-unique code, records prefixed templateSuffix create/update plus readback, and deactivates the setup gift card.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-notification-validation',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-gift-card-notification-validation-conformance.ts',
    purpose: 'Gift-card notification validation branches that fail before customer-visible notification dispatch.',
    requiredAuthScopes: ['read_gift_cards', 'write_gift_cards', 'read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-notification-validation.json`,
      'config/parity-specs/gift-cards/gift-card-notification-validation.json',
      'config/parity-specs/gift-cards/gift-card-notification-error-messages.json',
      'config/parity-requests/gift-cards/gift-card-notification-validation-customer-create.graphql',
      'config/parity-requests/gift-cards/gift-card-notification-validation-gift-card-create.graphql',
      'config/parity-requests/gift-cards/gift-card-notification-validation-deactivate.graphql',
    ],
    cleanupBehavior:
      'Creates disposable customers and validation-only gift cards, records failing notification responses, deactivates gift cards, and deletes customers.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-unrecordable-local-runtime-errors',
    scriptPath: 'scripts/capture-gift-card-unrecordable-local-runtime.ts',
    purpose:
      'Local-runtime fallback fixtures for gift-card entitlement-disabled and notify-disabled branches that cannot be constructed through the public conformance harness.',
    requiredAuthScopes: ['local-runtime'],
    fixtureOutputs: [
      `${LOCAL_RUNTIME_ROOT}gift-card-entitlement-disabled.json`,
      `${LOCAL_RUNTIME_ROOT}gift-card-create-notify.json`,
    ],
    cleanupBehavior:
      'No Shopify cleanup required; fixtures encode deterministic local-runtime fallback evidence for unrecordable branches.',
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
      'config/parity-requests/gift-cards/gift-card-transaction-validation-create.graphql',
      'config/parity-requests/gift-cards/gift-card-transaction-validation-deactivate.graphql',
      'config/parity-requests/gift-cards/gift-card-transaction-validation.graphql',
    ],
    cleanupBehavior:
      'Creates disposable active, expired, and deactivated gift cards; deactivates any setup cards not already deactivated during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-credit-debit-payload-shape',
    scriptPath: 'scripts/capture-gift-card-credit-debit-payload-shape-conformance.ts',
    purpose:
      'Gift-card credit/debit payload shape for typed transaction selections and schema rejection when giftCard is selected on transaction payloads.',
    requiredAuthScopes: [
      'read_gift_cards',
      'write_gift_cards',
      'read_gift_card_transactions',
      'write_gift_card_transactions',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-credit-debit-payload-shape.json`,
      'config/parity-specs/gift-cards/gift-card-credit-debit-payload-shape.json',
      'config/parity-requests/gift-cards/gift-card-credit-debit-payload-shape-create.graphql',
      'config/parity-requests/gift-cards/gift-card-credit-payload-shape.graphql',
      'config/parity-requests/gift-cards/gift-card-debit-payload-shape.graphql',
      'config/parity-requests/gift-cards/gift-card-credit-payload-gift-card-rejected.graphql',
      'config/parity-requests/gift-cards/gift-card-debit-payload-gift-card-rejected.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable gift card, records valid credit/debit payloads and validation-only invalid selections, then deactivates the setup gift card.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'gift-cards',
    captureId: 'gift-card-user-error-typename',
    scriptPath: 'scripts/capture-gift-card-user-error-typename-conformance.ts',
    purpose:
      'Gift-card mutation userErrors __typename values for typed create, credit, debit, deactivate, and notification payloads.',
    requiredAuthScopes: [
      'read_gift_cards',
      'write_gift_cards',
      'read_gift_card_transactions',
      'write_gift_card_transactions',
    ],
    fixtureOutputs: [
      `${CAPTURE_ROOT}gift-card-user-error-typename.json`,
      'config/parity-specs/gift-cards/gift-card-user-error-typename.json',
      'config/parity-requests/gift-cards/gift-card-user-error-typename.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable small-balance gift card, records validation-only mutation errors, then deactivates the setup gift card during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'GiftCardUpdatePayload.userErrors is generic UserError in the public schema and lacks a selectable code field, so update typename behavior is covered by schema evidence plus local runtime tests.',
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
      'fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/customers/customer-email-normalization.json',
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
      'config/parity-specs/customers/customerCreate-parity-plan.json',
      'config/parity-specs/customers/customerUpdate-parity-plan.json',
      'config/parity-specs/customers/customerDelete-parity-plan.json',
      `${CAPTURE_ROOT}customer-create-parity.json`,
      `${CAPTURE_ROOT}customer-update-parity.json`,
      `${CAPTURE_ROOT}customer-delete-parity.json`,
      'config/parity-requests/customers/customer-mutation-hydrate.graphql',
      'config/parity-requests/customers/customer-count-hydrate.graphql',
      'config/parity-requests/customers/customer-duplicate-hydrate.graphql',
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
    captureId: 'customer-create-name-identity',
    scriptPath: 'scripts/capture-customer-create-name-identity-conformance.ts',
    purpose:
      'customerCreate firstName-only, lastName-only, and blank-input identity precondition behavior with downstream reads.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-create-name-identity.json`,
      'config/parity-specs/customers/customer_create_name_identity.json',
      'config/parity-requests/customers/customer_create_name_identity.graphql',
      'config/parity-requests/customers/customer_create_name_identity_read.graphql',
    ],
    cleanupBehavior: 'Creates disposable firstName-only and lastName-only customers, then deletes them during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-email-normalization',
    scriptPath: 'scripts/capture-customer-email-normalization-conformance.ts',
    purpose:
      'Customer email whitespace stripping, RFC-style format validation, length cap, and normalized duplicate detection.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-email-normalization.json`,
      'config/parity-specs/customers/customer-email-normalization.json',
      'config/parity-requests/customers/customer-email-normalization-create.graphql',
      'config/parity-requests/customers/customer-email-normalization-read.graphql',
      'config/parity-requests/customers/customer-email-normalization-set.graphql',
      'config/parity-requests/customers/customer-email-normalization-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable customerCreate/customerSet records, records validation branches, and deletes created customers during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-tag-note-limits',
    scriptPath: 'scripts/capture-customer-tag-note-limits-conformance.ts',
    purpose: 'Customer tag splitting/deduplication and tags/note length cap validation codes.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-tag-note-limits-parity.json`,
      'config/parity-specs/customers/customerTagNoteLimits-parity.json',
      'config/parity-requests/customers/customer-tag-note-limits-create.graphql',
      'config/parity-requests/customers/customer-tag-note-limits-update.graphql',
      'config/parity-requests/customers/customer-tag-note-limits-set.graphql',
      'config/parity-requests/customers/customer-tag-note-limits-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable customers for successful tag normalization cases; validation-only branches return no customer, and created records are deleted during cleanup.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-phone-normalization',
    scriptPath: 'scripts/capture-customer-phone-normalization-conformance.mts',
    purpose:
      'Customer phone validation and normalization for formatted input, E.164 duplicate identity checks, invalid values, and length limits.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-phone-normalization.json`,
      'config/parity-specs/customers/customer-phone-normalization.json',
      'config/parity-requests/customers/customer-phone-normalization-create.graphql',
      'config/parity-requests/customers/customer-phone-normalization-set.graphql',
      'config/parity-requests/customers/customer-phone-normalization-update.graphql',
    ],
    cleanupBehavior:
      'Creates disposable customers for normalization and update branches; deletes remaining records after recording validation and duplicate probes.',
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
      'config/parity-specs/customers/customerUpdate-inline-consent-rejection.json',
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
    captureId: 'customer-address-input-validation',
    scriptPath: 'scripts/capture-customer-address-input-validation-conformance.mts',
    purpose: 'Customer address input length, HTML, URL, emoji, blank-address, and whitespace normalization behavior.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-address-input-validation.json`,
      'config/parity-specs/customers/customer-address-input-validation.json',
      'config/parity-requests/customers/customerInputValidation-create.graphql',
      'config/parity-requests/customers/customer-address-lifecycle-create-address.graphql',
      'config/parity-requests/customers/customer-address-lifecycle-update-address.graphql',
      'config/parity-requests/customers/customerSet-parity.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable customer, records address validation and normalization branches, then deletes it.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
    notes:
      'Dedicated customerAddressCreate and customerSet accept all-blank address strings after normalizing them to null, while nested CustomerInput addresses reject an all-blank entry.',
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
    captureId: 'customer-metafields-erasure-address-fidelity',
    scriptPath: 'scripts/capture-customer-metafields-erasure-address-fidelity.mts',
    purpose:
      'Customer create/read parity for multiple customer-owned metafields and Denmark address normalization, plus data-erasure hydration of a real unstaged customer.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'write_customer_data_erasure'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-metafields-erasure-address-fidelity.json`,
      'config/parity-specs/customers/customer-metafields-erasure-address-fidelity.json',
      'config/parity-requests/customers/customer-metafields-erasure-address-create.graphql',
      'config/parity-requests/customers/customer-metafields-erasure-address-read.graphql',
    ],
    cleanupBehavior:
      'Creates one disposable customer with multiple metafields and a Denmark address, requests data erasure, cancels erasure, then deletes the customer.',
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
    captureId: 'store-credit-unknown-id',
    environment: { SHOPIFY_CONFORMANCE_API_VERSION: '2026-04' },
    scriptPath: 'scripts/capture-store-credit-unknown-id-conformance.ts',
    purpose:
      'StoreCreditAccountCredit and StoreCreditAccountDebit missing-id userError envelopes for well-formed but nonexistent StoreCreditAccount, Customer, and CompanyLocation ids.',
    requiredAuthScopes: ['read_customers', 'read_companies', 'store credit account access'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}store-credit-account-unknown-id-user-errors.json`,
      'config/parity-specs/customers/store-credit-account-unknown-id-user-errors.json',
    ],
    cleanupBehavior: 'Uses fixed never-created IDs only; Shopify rejects before mutation so no cleanup is required.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-set',
    scriptPath: 'scripts/capture-customer-set-conformance.mts',
    purpose: 'customerSet upsert/identifier semantics.',
    requiredAuthScopes: ['read_customers', 'write_customers'],
    fixtureOutputs: [
      'fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/customers/customer-set-parity.json',
      'fixtures/conformance/local-runtime/2026-04/customers/customer-set-unknown-id-errors.json',
      'config/parity-specs/customers/customer-set-unknown-id-code.json',
      'config/parity-specs/customers/customer_set_unknown_id_errors.json',
    ],
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
      'config/parity-requests/customers/customer-merge-hydrate.graphql',
    ],
    cleanupBehavior: 'Creates disposable customers; merge consumes source records and cleanup removes leftovers.',
    expectedStatusChecks: DEFAULT_STATUS_CHECKS,
  },
  {
    domain: 'customers',
    captureId: 'customer-merge-selection',
    scriptPath: 'scripts/capture-customer-merge-selection-conformance.mts',
    purpose:
      'customerMerge resulting-customer selection rules across override, email-presence, account-state, and no-email branches.',
    requiredAuthScopes: ['read_customers', 'write_customers', 'read_customer_merge', 'write_customer_merge'],
    fixtureOutputs: [
      `${CAPTURE_ROOT}customer-merge-selection-rules.json`,
      'config/parity-specs/customers/customerMerge-selection-rules.json',
      'config/parity-requests/customers/customer-merge-selection-merge.graphql',
      'config/parity-requests/customers/customer-merge-selection-read-with-email.graphql',
      'config/parity-requests/customers/customer-merge-selection-read.graphql',
    ],
    cleanupBehavior:
      'Creates disposable customer pairs, sends one account invite for the account-state branch, merge consumes source records, and cleanup removes survivors.',
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
      'config/parity-specs/customers/customerEmailMarketingConsentUpdate-missing-opt-in-level.json',
      'config/parity-specs/customers/customerSmsMarketingConsentUpdate-disallowed-states-parity.json',
      'config/parity-specs/customers/customerSmsMarketingConsentUpdate-missing-opt-in-level.json',
      'config/parity-requests/customers/taggable-customer-hydrate.graphql',
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
      'config/parity-specs/customers/customer-order-summary-read-effects.json',
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
  const normalizedScript = script.toLowerCase();
  return conformanceCaptureIndex.find(
    (entry) => entry.captureId === script || entry.captureId === normalizedScript || entry.scriptPath === script,
  );
}

const invokedPath = process.argv[1] ? path.resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  runCli();
}
