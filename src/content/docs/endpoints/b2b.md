---
title: 'B2B Company, Contact, And Location Roots'
description: 'Coverage notes and fidelity boundaries for B2B Company, Contact, And Location Roots.'
---

This endpoint group tracks Shopify Admin GraphQL Business Customers roots for
companies, company contacts, company locations, roles, staff assignments,
company-location tax settings, and B2B address behavior.

## Current support and limitations

### Implemented local roots

<<<<<<< ours
The Rust operation registry marks the B2B roots below as locally implemented:
they route through `LOCAL_DISPATCH_ROOTS` and are answered without runtime
Shopify writes. This is not a support claim for the whole B2B domain.

Implemented read roots:

=======
The implemented read roots are:

>>>>>>> theirs
- `company`
- `companyLocation`

<<<<<<< ours
Implemented mutation roots:

- `companyAssignCustomerAsContact`
- `companyCreate`
=======
The implemented mutation roots are:

- `companyAddressDelete`
- `companyCreate`
- `companyLocationAssignAddress`
- `companyLocationAssignRoles`
- `companyLocationAssignStaffMembers`
- `companyLocationCreate`
- `companyLocationDelete`
- `companyLocationRemoveStaffMembers`
- `companyLocationRevokeRoles`
- `companyLocationsDelete`
>>>>>>> theirs
- `companyLocationTaxSettingsUpdate`
- `companyLocationUpdate`
- `companyUpdate`

<<<<<<< ours
Other B2B roots remain registry-known but unsupported until the proxy has
local lifecycle/read models and executable evidence for those roots.

### Local behavior

The Rust runtime keeps selected B2B behavior as local slices for ported parity
and runtime coverage. These slices stage only the modeled root and shape covered
by their checked-in request documents and tests; they do not promote the entire
B2B root family to supported status.

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

`companyLocationCreate`, `companyLocationUpdate`,
`companyLocationAssignAddress`, `companyAddressDelete`,
`companyLocationAssignRoles`, and `companyLocationRevokeRoles` stage the
location-side state needed by captured contact/location parity flows.
Location updates reject blank/whitespace names after HTML stripping, preserve
accepted names and buyer-experience fields that are covered by runtime tests;
address assignment/delete updates staged billing-address reads; location role
assign/revoke shares the same local role-assignment graph as contact-side
assignment roots.

`companyDelete` and `companiesDelete` stage local cascade deletion for company
records and their staged company locations only after a
deletable-status-style precheck. Staged orders, completed draft-order orders,
open draft orders, and CompanyLocation store-credit balances that reference the
target company block deletion with `FAILED_TO_DELETE`; bulk deletion returns
per-index errors and still deletes unblocked companies. Unknown bulk IDs return
`RESOURCE_NOT_FOUND`.

Fixture-backed read helpers cover stable B2B read shapes used as evidence,
including `company.customerSince`,
`CompanyContactRoleAssignment.companyContact`, and
`CompanyContactRoleAssignment.companyLocation`. These helpers are evidence for
the selected payloads, not a broad local B2B catalog implementation.

Older parity specs still describe rich B2B lifecycle behavior captured from
Shopify, including company/contact/location lifecycle, role-assignment cleanup,
address management, deletion blockers, bulk field paths, and staff-assignment
guardrails. Until the Rust registry and dispatcher expose those behaviors as
local staging roots, endpoint consumers should treat them as captured evidence
and porting targets rather than full current-domain support.
=======
Tracked but unimplemented B2B roots remain registry-only until they have their
own local lifecycle and downstream read-after-write model. This includes
`companies`, `companiesCount`, company-contact lifecycle roots,
company-main-contact roots, company delete roots, and
`companyContactSendWelcomeEmail`.

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
`["rolesToAssign", i]`; revoke returns indexed missing-assignment errors at
`["rolesToRevoke", i]`.

`companyLocationTaxSettingsUpdate` stages `taxExempt`, tax-exemption assignment
and removal, nullable input validation, and unknown-location user errors.
`companyLocationUpdate` also stages buyer-experience configuration fields for
the covered request shape, including `editableShippingAddress`,
`checkoutToDraft`, `paymentTermsTemplate`, and `deposit`.
>>>>>>> theirs

### Boundaries

- `companyContactSendWelcomeEmail` remains unsupported because it is an outbound
  customer-visible email side effect. The proxy has no no-send model for it.
- Company-contact lifecycle mutations are not fully implemented. Contacts and
  contact roles can be staged through nested setup on `companyCreate` for role
  assignment tests, but standalone contact create/update/delete/role roots
  remain unsupported.
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
<<<<<<< ours
- Unsupported B2B mutation handling must remain visible as passthrough or
  reject behavior according to runtime configuration unless a request matches a
  documented local runtime slice.
=======
- Unsupported B2B roots still use the configured unsupported mutation behavior
  and must remain visible as passthrough or reject events.
>>>>>>> theirs

### Evidence

- Registry status: `src/operation_registry.rs`
- Runtime coverage: `tests/graphql_routes/b2b.rs`
<<<<<<< ours
- Read and lifecycle parity specs: `config/parity-specs/b2b/*.json`
=======
- Address lifecycle parity: `config/parity-specs/b2b/b2b-location-address-management.json`
  and `config/parity-specs/b2b/location_assign_address_preserves_id.json`
- Staff validation parity:
  `config/parity-specs/b2b/staff_assign_unknown_user.json`,
  `config/parity-specs/b2b/staff_remove_unknown_assignment.json`, and
  `config/parity-specs/b2b/b2b-bulk-mutation-field-paths.json`
- Contact/location-role parity:
  `config/parity-specs/b2b/b2b-contact-location-assignments-tax.json` and
  `config/parity-specs/b2b/b2b-revoke-role-scope-preconditions.json`
>>>>>>> theirs
- Read and lifecycle fixtures: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/b2b/*.json` and `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/b2b/*.json`
- Root inventory fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`

### Validation

- `corepack pnpm lint`
- `corepack pnpm rust:test`
