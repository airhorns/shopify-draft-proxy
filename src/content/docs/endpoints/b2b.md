---
title: 'B2B Company, Contact, And Location Roots'
description: 'Coverage notes and fidelity boundaries for B2B Company, Contact, And Location Roots.'
---

This endpoint group tracks Shopify Admin GraphQL Business Customers roots for
companies, company contacts, company locations, roles, staff assignments,
company-location tax settings, and B2B address behavior.

## Current support and limitations

### Implemented local roots

The implemented read roots are:

- `companies`
- `company`
- `companyContact`
- `companyLocation`
- `companyLocations`

The implemented mutation roots are:

- `companyAddressDelete`
- `companyAssignCustomerAsContact`
- `companyAssignMainContact`
- `companiesDelete`
- `companyCreate`
- `companyContactAssignRole`
- `companyContactAssignRoles`
- `companyContactCreate`
- `companyContactDelete`
- `companyContactRemoveFromCompany`
- `companyContactRevokeRoles`
- `companyContactsDelete`
- `companyContactUpdate`
- `companyDelete`
- `companyLocationAssignAddress`
- `companyLocationAssignRoles`
- `companyLocationAssignStaffMembers`
- `companyLocationCreate`
- `companyLocationDelete`
- `companyLocationRemoveStaffMembers`
- `companyLocationRevokeRoles`
- `companyLocationsDelete`
- `companyLocationTaxSettingsUpdate`
- `companyLocationUpdate`
- `companyRevokeMainContact`
- `companyUpdate`

Tracked but unimplemented B2B roots remain registry-only until they have their
own local lifecycle and downstream read-after-write model. This includes
`companiesCount`, `companyContactRole`, and `companyContactSendWelcomeEmail`.

### Local behavior

Supported B2B mutations stage locally and retain the original raw mutation body
for `POST /__meta/commit` replay. Failed local validations are recorded in the
mutation log as failed and do not stage resource IDs.

The local B2B graph stores staged companies, company locations, company
addresses embedded on locations, company contacts, contact roles,
location-role assignments, and location-staff assignments. `company(id:)`,
`companyLocation(id:)`, and `companyLocations` expand that staged graph for
read-after-write, including nested `locations`, `contacts`, `contactRoles`,
`roleAssignments`, and `staffMemberAssignments` connections. LiveHybrid reads
that do not target staged B2B IDs continue to use the existing upstream or
fixture-backed read path.

`companyCreate` and `companyUpdate` stage company identity fields, validate
company name length, strip HTML from accepted names, validate `externalId`
character set, length, and duplicates, reject HTML or overlong notes, and
reject `companyUpdate(input.customerSince)` without mutating the staged company.
`companyCreate` can also stage nested company location, contact, and contact
role setup when those input objects are present.

`companyContactCreate`, `companyContactUpdate`, `companyContactDelete`,
`companyContactsDelete`, and `companyContactRemoveFromCompany` stage the
company-contact lifecycle and keep company `contactIds`, contact customer data,
role assignments, and downstream contact reads in sync. Deleting or removing
the current main contact clears the company's `mainContact`. `companyContactCreate`
requires an email-backed customer reference; omitting `input.email` returns
`INVALID` at `["input"]` without staging a contact or customer. The nested
`companyCreate(input.companyContact)` path applies the same requirement at
`["input", "companyContact"]` before staging any company, location, role, contact,
or assignment rows.

`companyAssignMainContact` and `companyRevokeMainContact` stage the company's
single `mainContactId` pointer and derive each contact's `isMainContact` from
that pointer. Assigning an unknown contact returns `RESOURCE_NOT_FOUND`;
assigning a contact that exists on another staged company returns
`INVALID_INPUT` at `["companyContactId"]` without mutating the target company.

`companyLocationCreate`, `companyLocationUpdate`, `companyLocationDelete`, and
`companyLocationsDelete` stage the company-location lifecycle. Location create
uses the Shopify-like fallback chain `input.name` -> `shippingAddress.address1`
-> company name. A present blank `companyLocationUpdate(input.name)` returns a
`BLANK` user error without mutating the staged location. Bulk deletion returns
per-index `RESOURCE_NOT_FOUND` errors at `["companyLocationIds", i]` while still
deleting valid staged IDs.

`companyLocationAssignAddress` updates the requested address slots locally,
rejects duplicate `addressTypes` with `INVALID_INPUT`, and preserves the
existing `CompanyAddress` GID when reassigning the same address type.
`companyAddressDelete` clears any staged location slot that references the
deleted address; when billing and shipping share the same address it clears both
slots and resets `billingSameAsShipping` to `false`.

`companyLocationAssignStaffMembers` and
`companyLocationRemoveStaffMembers` stage staff assignment rows. Assignment
dedups already-assigned staff, enforces a maximum of 10 staff members per
location, and returns indexed `RESOURCE_NOT_FOUND` errors at
`["staffMemberIds", i]` for unknown staff IDs. Removal returns indexed
`RESOURCE_NOT_FOUND` errors at
`["companyLocationStaffMemberAssignmentIds", i]` for unknown assignment IDs.

`companyLocationAssignRoles` and `companyLocationRevokeRoles` stage
company-contact role assignments for locations. Assignment validates that the
staged contact and role exist and returns indexed `RESOURCE_NOT_FOUND` errors at
`["rolesToAssign", i]`. Bulk role assignment also enforces Shopify's one
role-per-contact-per-location rule: an entry whose contact already holds any
role at the target location returns indexed `LIMIT_REACHED` at
`["rolesToAssign", i]`, while valid sibling entries in the same request still
stage and return in `roleAssignments`. Revoke returns indexed
missing-assignment errors at `["rolesToRevoke", i]`.

`companyLocationTaxSettingsUpdate` stages `taxExempt`, `taxRegistrationId`, and
tax-exemption assignment/removal under `CompanyLocation.taxSettings`. Exemption
updates apply against the current staged location set by removing
`exemptionsToRemove` and then appending new `exemptionsToAssign` values without
inventing defaults. Omitting `taxExempt` or `taxRegistrationId` preserves the
current staged value; literal `taxExempt: null` and variable `taxExempt: null`
return `INVALID_INPUT`, while an unbound optional `$taxExempt` variable is
treated as omitted.
`companyLocationUpdate` also stages buyer-experience configuration fields for
the covered request shape, including `editableShippingAddress`,
`checkoutToDraft`, `paymentTermsTemplate`, and `deposit`.

### Boundaries

- `companyContactSendWelcomeEmail` remains unsupported because it is an outbound
  customer-visible email side effect. The proxy has no no-send model for it.
- Staff assignment does not synthesize a full staff catalog or support staff
  catalog reads. The local model accepts structurally valid staged-test staff
  IDs for assignment rows and returns Shopify-like per-index errors for unknown
  staff or assignment IDs.
- Validation-only B2B parity specs prove guardrail payloads and no-stage
  behavior for those inputs only. They do not make the corresponding mutation
  roots generally supported.
- Generic `node(id:)` / `nodes(ids:)` dispatch for B2B-only IDs is limited to
  fixture-backed or tail-helper evidence. Do not infer complete Node support for
  companies, contacts, locations, addresses, staff assignments, or catalogs.
- Unsupported B2B roots still use the configured unsupported mutation behavior
  and must remain visible as passthrough or reject events.

### Evidence

- Registry status: `src/operation_registry.rs`
- Runtime coverage: `tests/graphql_routes/b2b.rs`
- Company contact and main-contact lifecycle parity:
  `config/parity-specs/b2b/b2b-company-contact-main-delete.json`
- Contact missing-email validation parity:
  `config/parity-specs/b2b/b2b-contact-missing-email-validation.json`
- Address lifecycle parity: `config/parity-specs/b2b/b2b-location-address-management.json`
  and `config/parity-specs/b2b/location_assign_address_preserves_id.json`
- Staff validation parity:
  `config/parity-specs/b2b/staff_assign_unknown_user.json`,
  `config/parity-specs/b2b/staff_remove_unknown_assignment.json`, and
  `config/parity-specs/b2b/b2b-bulk-mutation-field-paths.json`
- Contact/location-role parity:
  `config/parity-specs/b2b/b2b-contact-location-assignments-tax.json` and
  `config/parity-specs/b2b/b2b-revoke-role-scope-preconditions.json`
- Company-location tax-settings parity:
  `config/parity-specs/b2b/b2b-company-location-tax-settings-sequential.json`
- Bulk duplicate role-assignment parity:
  `config/parity-specs/b2b/b2b-bulk-role-assign-duplicates.json`
- Read and lifecycle fixtures: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/b2b/*.json` and `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/b2b/*.json`
- Root inventory fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`

### Validation

- `corepack pnpm lint`
- `corepack pnpm rust:test`
- `corepack pnpm parity -- --spec config/parity-specs/b2b/b2b-company-location-tax-settings-sequential.json`
