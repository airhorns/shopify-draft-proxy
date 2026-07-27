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
- `companiesCount`
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
`companyContactRole` and `companyContactSendWelcomeEmail`.

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

The local B2B graph separates upstream-observed LiveHybrid records from staged
mutation overlays for companies, company locations, company addresses, company
contacts, customers, contact roles, location-role assignments, and
location-staff assignments. When a
LiveHybrid `companies`, `companiesCount`, or `companyLocations` read enters
local B2B handling, the proxy observes upstream connection rows into the base
catalog and observes `companiesCount` Count responses as count baselines keyed
by count arguments, then answers from one effective graph. Staged creates append
to the base catalog, staged updates replace matching base rows by ID, and staged
deletes tombstone base or staged rows so repeated upstream reads do not
resurrect them. `companiesCount` starts from the observed upstream count
baseline instead of deriving the total from a page-limited `companies`
connection. Upstream observations do not make staged state dirty; only
successful local mutation effects enter the staged overlay. `company(id:)`,
`companyLocation(id:)`, `companies`,
`companiesCount`, and `companyLocations` expand that effective graph for
read-after-write, including nested `locations`, `contacts`, `contactRoles`,
`roleAssignments`, and `staffMemberAssignments` connections. `Company.orders`
and `CompanyLocation.orders` expose the same staged orders that feed
`ordersCount`, `orderCount`, and `totalSpent`; `Company.draftOrders` and
`CompanyLocation.draftOrders` expose matching staged draft orders. These nested
order connections honor cursor windows and return empty connection objects when
no staged records match.

Cold LiveHybrid `company(id:)`, `companyContact(id:)`, and
`companyLocation(id:)` reads forward upstream and observe the returned entity
and relationships before local projection. An observed company, location, or
contact does not make its resource family or sibling relationship catalog
complete: singular reads without a staged overlay remain authoritative upstream
reads. Once an entity has a staged update or tombstone, that overlay takes
precedence over later upstream observations.

Before a locally supported B2B mutation makes not-found, uniqueness, ownership,
cardinality, or membership decisions in LiveHybrid mode, it deduplicates
referenced real Shopify IDs and resolves them through batched `nodes(ids:)`
queries. Each mutation selects only the scalar and relationship evidence it
needs. External-ID and contact-email checks use targeted upstream searches;
location cardinality uses a bounded two-row probe; role membership uses an
indexed one-row filter; and the staff-assignment limit uses a bounded 11-row
probe. Only explicit contact `revokeAll` handling exhausts that contact's role
assignment connection. Direct `CompanyAddress` identity establishes whether an
address can be deleted without scanning the company-location catalog; observed
owner indexes clear known slots, and an address tombstone masks a matching slot
if its owner is observed later. Partial connection observations are marked as
partial and are not used as whole-catalog absence evidence. These prerequisite
reads hydrate the observed base graph only; the original mutation remains local
and is not sent upstream before `POST /__meta/commit`.

Local B2B list connections use the shared staged-connection path for filtering,
sorting, `reverse`, cursor windows, and `pageInfo`. `companies` supports
field-scoped query terms for `id`, `name`, and `external_id`; `companyLocations`
supports `id`, `name`, `external_id`, and `company_id`. Unsupported or
unparseable query terms are treated as unsupported local filters and return an
empty staged connection rather than matching all staged records. `companies`,
`companyLocations`, `Company.contacts`, `Company.locations`, location/contact
`roleAssignments`, `Company.contactRoles`, and
`CompanyLocation.staffMemberAssignments` honor local `sortKey` and `reverse`
for the modeled staged fields, defaulting to ID order when a sort key has no
modeled field in local state. `companiesCount` returns the effective company
count selected through the Shopify `Count` object shape after applying modeled
filters, staged inserts/deletes/updates, and any `limit`; upstream `AT_LEAST`
precision remains `AT_LEAST` when the exact live total is not knowable from the
count response. `companies(first:, query:)` and `companiesCount` are answered
from the local B2B graph only after company state has been staged or hydrated in
the current session. Cold LiveHybrid company connection/count reads forward
upstream unchanged so real store companies are visible before local B2B writes
occur. `Company.lifetimeDuration` is derived from staged or hydrated
`customerSince`, falling back to the local `createdAt` timestamp from company
creation, and is returned even when the company has no orders.

`companyCreate` and `companyUpdate` stage company identity fields, validate
company name length, strip HTML from accepted names, validate `externalId`
character set, length, and duplicates, validate note length while preserving
note HTML verbatim, and reject `companyUpdate(input.customerSince)` without
mutating the staged company.
`companyCreate` can also stage nested company location, contact, and contact
role setup when those input objects are present. Its nested
`input.companyLocation` name follows Shopify's create-time fallback of
`companyLocation.name` -> company name, without using
`shippingAddress.address1`.

`companyContactCreate`, `companyContactUpdate`, `companyContactDelete`,
`companyContactsDelete`, and `companyContactRemoveFromCompany` stage the
company-contact lifecycle and keep company `contactIds`, contact customer data,
role assignments, and downstream contact reads in sync. Contact and company
location tombstones suppress their assignment children during effective reads,
so deletes do not need to enumerate unrelated assignment pages. Local-format contact
phone input normalizes through the shop country observed from local state or a
LiveHybrid query-only shop-country hydrate; if no country context is available,
the proxy does not assume a default calling code. Deleting or removing the
current main contact clears the company's `mainContact`. `companyContactCreate`
stores `title` verbatim, including HTML, but rejects HTML in `firstName` or
`lastName` with generic `INVALID_INPUT` at `["input"]`. It requires an
email-backed customer reference; omitting `input.email` returns `INVALID` at
`["input"]` without staging a contact or customer. The nested
`companyCreate(input.companyContact)` path applies contact validation under
`["input", "companyContact"]` before staging any company, location, role,
contact, or assignment rows. Company contacts preserve explicit `title` input
and otherwise store `title: null`, including nested create. Contact locale is
explicit input when supplied; otherwise contact-create paths use the shop's
primary locale, and `companyAssignCustomerAsContact` uses the assigned customer's
locale before falling back to the shop primary locale.

In LiveHybrid mode, `companyAssignCustomerAsContact` resolves an unknown
persisted company and customer through narrow query-only reads before applying
local validation. The mutation itself remains local until commit replay. A
missing company returns `RESOURCE_NOT_FOUND` at `["companyId"]` before customer
validation; a missing customer returns the corresponding error at
`["customerId"]`. Successful assignment stages the contact and relationship so
`Company.contacts`, `companyContact(id:)`, and
`Customer.companyContactProfiles` expose the same customer-company link without
an earlier client hydration read.

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
`companyAddressDelete` clears any indexed location slot that references the
deleted address and records an address tombstone; when billing and shipping
share the same address it clears both slots and resets
`billingSameAsShipping` to `false`.

`companyLocationAssignStaffMembers` and
`companyLocationRemoveStaffMembers` stage staff assignment rows. Assignment
dedups already-assigned staff, enforces a maximum of 10 observed staff members
per location, resolves real direct-ID inputs through query-only LiveHybrid
hydration, and returns indexed `RESOURCE_NOT_FOUND` errors at
`["staffMemberIds", i]` for unresolved staff IDs. Removal returns indexed
`RESOURCE_NOT_FOUND` errors at
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
updates apply against the current staged or LiveHybrid-hydrated location by
removing `exemptionsToRemove` and then appending new `exemptionsToAssign`
values without inventing defaults. Omitting `taxExempt` or `taxRegistrationId`
preserves the current value; literal `taxExempt: null` and variable
`taxExempt: null` return `INVALID_INPUT`, while an unbound optional
`$taxExempt` variable is treated as omitted. Supplying no tax-setting knobs is a
successful no-op that returns the unchanged company location. Invalid
`TaxExemption` variables and inline literals are top-level GraphQL errors before
staging; inline literal messages echo the submitted enum literal in Shopify's
field-level coercion shape rather than a fixed fallback value or suggestion. In
LiveHybrid mode, `companyLocationTaxSettingsUpdate`, `companyLocationUpdate`,
and company-location store-credit ownership checks hydrate an existing upstream
`CompanyLocation` before staging local effects when it is not already in the
local graph; snapshot mode only accepts locations already present in local
state.
`companyLocationUpdate` also stages buyer-experience configuration fields for
the covered request shape, including `editableShippingAddress`,
`checkoutToDraft`, `paymentTermsTemplate`, and `deposit`. Deposit input is
stored as a `DepositPercentage` object with the supplied `percentage` value.

### Boundaries

- `companyContactSendWelcomeEmail` remains unsupported because it is an outbound
  customer-visible email side effect. The proxy has no no-send model for it.
- Staff assignment does not synthesize a full staff catalog, accept arbitrary
  numeric StaffMember GIDs, or support staff catalog reads. LiveHybrid mutation
  prerequisites can resolve referenced staff members through batched
  `nodes(ids:)` queries;
  unresolved staff or assignment IDs return Shopify-like per-index errors.
- Validation-only B2B parity specs prove guardrail payloads and no-stage
  behavior for those inputs only. They do not make the corresponding mutation
  roots generally supported.
- Generic `node(id:)` / `nodes(ids:)` dispatch for B2B-only IDs is limited to
  fixture-backed or tail-helper evidence. Do not infer complete Node support for
  companies, contacts, locations, addresses, staff assignments, or catalogs.
- Unsupported B2B roots still use the configured unsupported mutation behavior
  and must remain visible as passthrough or reject events.
