---
title: 'B2B Company, Contact, And Location Roots'
description: 'Coverage notes and fidelity boundaries for B2B Company, Contact, And Location Roots.'
---

This endpoint group tracks Shopify Admin GraphQL Business Customers roots for
companies, company contacts, company locations, roles, staff assignments,
company-location tax settings, and B2B address behavior.

## Current support and limitations

### Implemented local roots

The Rust runtime locally stages the company-contact and contact-role assignment
family. Supported mutations append replay-ready log entries with the original
raw GraphQL request and compute responses from staged B2B company, contact,
customer-reference, main-contact, location, role, and role-assignment state.

The local read roots for staged B2B contact state are:

- `company`
- `companyContact`
- `companyContactRole`
- `companyLocation`
- `companyLocations`
- `node(id:)` for staged B2B contact, contact-role, location, address, and
  role-assignment IDs

The supported local mutation roots for staged contact and role-assignment
lifecycle behavior are:

- `companyAssignCustomerAsContact`
- `companyAssignMainContact`
- `companyContactAssignRole`
- `companyContactAssignRoles`
- `companyContactCreate`
- `companyContactDelete`
- `companyContactRemoveFromCompany`
- `companyContactRevokeRole`
- `companyContactRevokeRoles`
- `companyContactsDelete`
- `companyContactUpdate`
- `companyRevokeMainContact`

Additional local B2B setup and support slices are implemented for captured
company-contact parity flows:

- `companyCreate`
- `companyUpdate`
- `companyLocationCreate`
- `companyLocationUpdate`
- `companyLocationAssignAddress`
- `companyLocationAssignStaffMembers`
- `companyAddressDelete`
- `companyLocationAssignRoles`
- `companyLocationDelete`
- `companyLocationRemoveStaffMembers`
- `companyLocationRevokeRoles`
- `companyLocationTaxSettingsUpdate`
- `companyLocationsDelete`

The registry-only read roots are:

- `companies`
- `companiesCount`

The registry-only mutation roots are:

- `companiesDelete`
- `companyContactSendWelcomeEmail`
- `companyDelete`

### Local behavior

The Rust runtime keeps selected B2B behavior as staged local slices backed by
parity and runtime coverage. The company-contact and contact-role assignment
family is root-field dispatched and not gated to a specific parity document.
Other B2B company/location roots remain narrower local slices or registry-only
coverage-map entries until their full lifecycle behavior is modeled.

`companyCreate` and `companyUpdate` have a local slice for company identity
fields. That slice stages synthetic company records, preserves
read-after-write through `company(id:)` for staged IDs, validates blank and
overlong names, strips HTML from accepted names, validates `externalId`
character set, length, and duplicate values, rejects HTML/overlong notes, and
rejects `companyUpdate(input.customerSince)` without changing the staged
company. Company creation also seeds the default contact role used by staged
contact-role assignment mutations. Successful staged mutations append
replay-ready log entries with the original raw GraphQL request.

`companyContactCreate`, `companyContactUpdate`, `companyContactDelete`,
`companyContactsDelete`, `companyAssignCustomerAsContact`,
`companyContactRemoveFromCompany`, `companyAssignMainContact`,
`companyRevokeMainContact`, `companyContactAssignRole`,
`companyContactAssignRoles`, `companyContactRevokeRole`, and
`companyContactRevokeRoles` stage a local contact graph. Staged contacts are
readable through `companyContact(id:)`; `Company.mainContact` and
`Company.contacts` reflect assign/revoke/delete changes; role assignments are
readable from staged contacts and company locations. Contact title/name
validation rejects HTML with `CONTAINS_HTML_TAGS` and >255-character free-text
values with `TOO_LONG`. Bulk contact delete, role assign, and role revoke
payloads preserve Shopify-style per-index user-error field paths.

`companyLocationTaxSettingsUpdate` has a local tax-settings slice for required
and nullable input handling, invalid `TaxExemption` enum coercion, assignment
and removal of exemptions, `taxRegistrationId`, `taxExempt`, and downstream
payload projection. Validation failures return B2B `userErrors` and mark the
log entry as failed when no state is staged; invalid `TaxExemption` values are
rejected as top-level GraphQL coercion errors before staging.

`companyLocationCreate`, `companyLocationUpdate`, `companyLocationDelete`,
`companyLocationsDelete`, `companyLocationAssignAddress`, `companyAddressDelete`,
`companyLocationAssignStaffMembers`, `companyLocationRemoveStaffMembers`,
`companyLocationAssignRoles`, and `companyLocationRevokeRoles` stage
location-side state needed by captured contact/location parity flows. Location
create uses the Shopify-like fallback chain from `input.name` to
`shippingAddress.address1` to company name; blank location update names are
rejected without mutating staged state. Address assignment rejects duplicate
address types and preserves existing `CompanyAddress` IDs; address delete clears
shared shipping/billing slots and resets `billingSameAsShipping`. Staff
assignment dedups already-assigned staff, enforces the 10-staff cap, and
returns per-index `RESOURCE_NOT_FOUND` or `LIMIT_REACHED` paths. Location role
assign/revoke shares the same local role-assignment graph as contact-side
assignment roots.

Fixture-backed read helpers cover stable B2B read shapes used as evidence,
including `company.customerSince`,
`CompanyContactRoleAssignment.companyContact`, and
`CompanyContactRoleAssignment.companyLocation`. These helpers are evidence for
the selected payloads, not a broad local B2B catalog implementation.

Parity specs also describe richer B2B lifecycle behavior captured from Shopify,
including deletion blockers beyond the currently modeled contact/location graph.
Endpoint consumers should treat those remaining roots as captured evidence
rather than current local support until their own lifecycle behavior is modeled.

### Boundaries

- `companyContactSendWelcomeEmail` remains unsupported because it is an outbound
  customer-visible email side effect. The proxy has no no-send model for it.
- Staff assignment does not synthesize a staff catalog. Unknown staff and
  assignment IDs are covered by validation evidence, while authorized
  staff-catalog reads remain access-scope dependent.
- Validation-only B2B parity specs prove guardrail payloads and no-stage
  behavior for those inputs only. For B2B roots outside the implemented
  contact/location/role/staff slices, they do not make the corresponding
  mutation roots generally supported.
- Generic `node(id:)` dispatch for B2B-only IDs is limited to the staged B2B IDs
  named above. `nodes(ids:)` and staff/catalog Node hydration remain outside the
  current B2B support surface.
- B2B roots outside the implemented contact/location/role/staff slices still
  use visible passthrough or reject behavior according to runtime configuration
  unless a request matches a documented local runtime slice.

### Evidence

- Registry status: `src/operation_registry.rs`
- Runtime coverage: `tests/graphql_routes/b2b.rs`
- Read and lifecycle parity specs: `config/parity-specs/b2b/*.json`
- Read and lifecycle fixtures: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/b2b/*.json` and `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/b2b/*.json`
- Root inventory fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`

### Validation

- `corepack pnpm lint`
- `corepack pnpm rust:test`
