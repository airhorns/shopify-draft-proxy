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

Bulk B2B mutations follow Shopify's app/API-client request-level batch cap:
more than 50 entries returns a single top-level `LIMIT_REACHED` user error with
the bulk argument as `field`, before parent lookup or per-entry validation, and
does not mutate staged state. The local B2B dispatch path models app-facing
Admin API callers; there is no first-party merchant-admin local bypass in the
proxy, so the cap applies even when the optional
`x-shopify-draft-proxy-api-client-id` identity header is absent.

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
role setup when those input objects are present. Its nested
`input.companyLocation` name follows Shopify's create-time fallback of
`companyLocation.name` -> company name, without using
`shippingAddress.address1`.

`companyContactCreate`, `companyContactUpdate`, `companyContactDelete`,
`companyContactsDelete`, and `companyContactRemoveFromCompany` stage the
company-contact lifecycle and keep company `contactIds`, contact customer data,
role assignments, and downstream contact reads in sync. Deleting or removing
the current main contact clears the company's `mainContact`. `companyContactCreate`
requires an email-backed customer reference; omitting `input.email` returns
`INVALID` at `["input"]` without staging a contact or customer. The nested
`companyCreate(input.companyContact)` path applies the same requirement at
`["input", "companyContact"]` before staging any company, location, role, contact,
or assignment rows. Company contacts preserve explicit `title` input and
otherwise store `title: null`, including nested create. Contact locale is
explicit input when supplied; otherwise contact-create paths use the shop's
primary locale, and `companyAssignCustomerAsContact` uses the assigned
customer's locale before falling back to the shop primary locale.

`companyAssignMainContact` and `companyRevokeMainContact` stage the company's
single `mainContactId` pointer and derive each contact's `isMainContact` from
that pointer. Assigning an unknown contact returns `RESOURCE_NOT_FOUND`;
assigning a contact that exists on another staged company returns
`INVALID_INPUT` at `["companyContactId"]` without mutating the target company.

`companyContactRevokeRole` and `companyContactRevokeRoles` validate the parent
contact before revoking assignments. Bulk contact revocation rejects empty
`roleAssignmentIds` when `revokeAll` is false, emits indexed
`RESOURCE_NOT_FOUND` errors at `["roleAssignmentIds", i]` for assignments that
are missing or belong to another contact, and still revokes valid siblings.

`companyLocationCreate`, `companyLocationUpdate`, `companyLocationDelete`, and
`companyLocationsDelete` stage the company-location lifecycle. Location create
rejects standalone address-only input with Shopify-like `NO_INPUT` and no staged
location. When standalone create includes a non-address location attribute, the
name fallback remains `input.name` -> `shippingAddress.address1` -> company name.
The nested `companyCreate(input.companyLocation)` default location does not use
`shippingAddress.address1` as the name fallback; it falls back from `input.name`
to the company name. A present blank `companyLocationUpdate(input.name)` returns
a `BLANK` user error without mutating the staged location. Bulk deletion returns
per-index `RESOURCE_NOT_FOUND` errors at `["companyLocationIds", i]` while still
deleting valid staged IDs. Location locale is explicit input when supplied and
otherwise uses the shop's primary locale. Location phone normalization uses the
input shipping or billing address country, then existing location address
country on updates, then the shop country; bare digit-shaped phone input without
a usable country context returns the local invalid-phone branch instead of
assuming a North American calling code.

`companyLocationAssignAddress` updates the requested address slots locally,
rejects duplicate `addressTypes` with `INVALID_INPUT`, and preserves the
existing `CompanyAddress` GID when reassigning the same address type.
`companyAddressDelete` clears any staged location slot that references the
deleted address; when billing and shipping share the same address it clears both
slots and resets `billingSameAsShipping` to `false`.

`companyLocationAssignStaffMembers` and
`companyLocationRemoveStaffMembers` stage staff assignment rows. Assignment
dedups already-assigned staff, enforces a maximum of 10 observed staff members
per location, and returns indexed `RESOURCE_NOT_FOUND` errors at
`["staffMemberIds", i]` for staff IDs that are not present in already observed
staff-assignment state. Removal returns indexed `RESOURCE_NOT_FOUND` errors at
`["companyLocationStaffMemberAssignmentIds", i]` for unknown assignment IDs.

`companyLocationAssignRoles` and `companyLocationRevokeRoles` stage
company-contact role assignments for locations. Assignment validates that the
staged contact and role exist and returns indexed `RESOURCE_NOT_FOUND` errors at
`["rolesToAssign", i]`. Bulk role assignment also enforces Shopify's one
role-per-contact-per-location rule: an entry whose contact already holds any
role at the target location returns indexed `LIMIT_REACHED` at
`["rolesToAssign", i]`, while valid sibling entries in the same request still
stage and return in `roleAssignments`. Revoke validates the parent location,
returns `RESOURCE_NOT_FOUND` at `["companyLocationId"]` when it is missing, and
returns indexed `RESOURCE_NOT_FOUND` errors at `["rolesToRevoke", i]` for
assignments that are missing or belong to another location.

`companyLocationTaxSettingsUpdate` stages `taxExempt`, `taxRegistrationId`, and
tax-exemption assignment/removal under `CompanyLocation.taxSettings`. Exemption
updates apply against the current staged location set by removing
`exemptionsToRemove` and then appending new `exemptionsToAssign` values without
inventing defaults. Omitting `taxExempt` or `taxRegistrationId` preserves the
current staged value; literal `taxExempt: null` and variable `taxExempt: null`
return `INVALID_INPUT`, while an unbound optional `$taxExempt` variable is
treated as omitted. Supplying no tax-setting knobs is a successful no-op that
returns the unchanged company location.
`companyLocationUpdate` also stages buyer-experience configuration fields for
the covered request shape, including `editableShippingAddress`,
`checkoutToDraft`, `paymentTermsTemplate`, and `deposit`. Deposit input is
stored as a `DepositPercentage` object with the supplied `percentage` value.

### Boundaries

- `companyContactSendWelcomeEmail` remains unsupported because it is an outbound
  customer-visible email side effect. The proxy has no no-send model for it.
- Staff assignment does not synthesize a full staff catalog, accept arbitrary
  numeric StaffMember GIDs, or support staff catalog reads. The current
  conformance token receives `Access denied for staffMembers field`, so local
  assignment validity is limited to staff IDs already observed through staged
  assignment state; unknown staff or assignment IDs return Shopify-like
  per-index errors.
- Validation-only B2B parity specs prove guardrail payloads and no-stage
  behavior for those inputs only. They do not make the corresponding mutation
  roots generally supported.
- Generic `node(id:)` / `nodes(ids:)` dispatch for B2B-only IDs is limited to
  fixture-backed or tail-helper evidence. Do not infer complete Node support for
  companies, contacts, locations, addresses, staff assignments, or catalogs.
- Unsupported B2B roots still use the configured unsupported mutation behavior
  and must remain visible as passthrough or reject events.
