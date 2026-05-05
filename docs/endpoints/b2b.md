# B2B Company, Contact, And Location Roots

## Current support and limitations

### Supported reads

HAR-302 adds snapshot/parity support for these B2B Admin GraphQL roots:

- `companies`
- `companiesCount`
- `company`
- `companyContact`
- `companyContactRole`
- `companyLocation`
- `companyLocations`

The local model stores companies, company contacts, company contact roles, and
company locations as normalized in-memory records. Company catalog reads expose
captured scalar fields, nested contact/location/role connections, count objects,
`mainContact`, and `defaultRole`. Singular unknown-ID reads return `null`, and
empty snapshot catalogs return empty connections/count zero.

Snapshot-mode reads resolve locally from the effective B2B graph and use the
shared connection helpers for `nodes`, `edges`, cursor windows, and selected
`pageInfo` fields on top-level `companies` / `companyLocations` and nested
company contact/location/role connections. The local cursor strings are stable
synthetic cursors; captured Shopify cursors remain opaque and should not be
treated as semantically meaningful.

`companies(query:)` uses the shared Shopify Admin search helpers for the
captured local subset (`name`, `id`, `external_id` / `externalId`, and free-text
name/external-id terms). Shopify 2026-04 rejects `companiesCount(query:)`, so
local `companiesCount` intentionally remains an unfiltered aggregate.

Live-hybrid B2B reads currently pass through to Shopify rather than hydrating a
local B2B overlay. This preserves Shopify's access behavior for shops or tokens
without B2B/read-companies access, including field-level `ACCESS_DENIED`
responses, while snapshot mode remains the local no-upstream evidence path.

### Supported mutations

HAR-363 promotes B2B lifecycle roots from registry blockers to local staging.
Supported mutations synthesize Shopify-like payloads, append the original raw
GraphQL request to the mutation log for commit replay, and do not write to
Shopify at runtime:

- company lifecycle: `companyCreate`, `companyUpdate`, `companyDelete`,
  `companiesDelete`
- contact lifecycle and assignment: `companyContactCreate`,
  `companyContactUpdate`, `companyContactDelete`, `companyContactsDelete`,
  `companyAssignCustomerAsContact`, `companyContactRemoveFromCompany`
- main contact: `companyAssignMainContact`, `companyRevokeMainContact`
- contact/location role assignment: `companyContactAssignRole`,
  `companyContactAssignRoles`, `companyContactRevokeRole`,
  `companyContactRevokeRoles`, `companyLocationAssignRoles`,
  `companyLocationRevokeRoles`
- location lifecycle and settings: `companyLocationCreate`,
  `companyLocationUpdate`, `companyLocationDelete`, `companyLocationsDelete`,
  `companyLocationAssignAddress`, `companyAddressDelete`,
  `companyLocationAssignStaffMembers`,
  `companyLocationRemoveStaffMembers`,
  `companyLocationTaxSettingsUpdate`

The staged model stores companies, contacts, system contact roles, locations,
role assignments, location staff assignments, address payloads, and location tax
settings in the normalized in-memory B2B buckets. Subsequent `company`,
`companyContact`, `companyLocation`, `companyLocations`, `companies`, and
`companiesCount` reads observe the staged graph, including deletions.
Contacts created from `companyCreate(input.companyContact)` or
`companyContactCreate(input.email)` keep a contact-local synthetic customer
reference so downstream B2B `CompanyContact.customer { id }` reads match
Shopify's company/customer-contact relationship without broadening customer
catalog state. `companyAssignCustomerAsContact` stores the provided customer ID
as that contact reference only after resolving the customer from the effective
local customer registry. It rejects unknown customers, customers without an
email address, duplicate customer/contact assignments on the same company, and
companies that have reached the 10,000-contact cap. Main-contact lifecycle
stores a single `Company.mainContactId` pointer; returned
`CompanyContact.isMainContact` values are derived from that pointer.
`companyRevokeMainContact` clears only the company pointer and downstream
`Company.mainContact` reads return `null`, matching the captured Shopify
2026-04 behavior. `companyAssignMainContact` returns `INVALID_INPUT` when the
provided contact belongs to a different company.

Contact create/update inputs are prepared before staging to mirror Shopify's
B2B contact input handling. Supported local paths normalize valid phone numbers
to E.164 using the effective shop country code when the input omits a leading
country code, default created contact locales from the primary shop locale,
store input `note` as the contact `notes` attribute, and expose both `note` and
`notes` read selections from that value for compatibility with existing
captures. Invalid phone strings return `INVALID`; malformed locale tags return
Shopify's captured `INVALID` locale-format user error; notes containing HTML
tags return `CONTAINS_HTML_TAGS`; duplicate effective contact email or
normalized phone values return Shopify's captured `TAKEN` user error code with
the relevant `input.email` or `input.phone` field path.
`companyAssignCustomerAsContact` currently has only `companyId` and
`customerId` arguments in the checked-in Admin schema, so the local handler
defaults the created contact locale and derives the contact customer payload
from the resolved local `Customer` record instead of synthesizing an arbitrary
customer shape.

HAR-446 captured a fidelity trap in the company-create path: when
`companyCreate` creates both a main contact and a default company location,
Shopify automatically assigns that contact the `Ordering only` role for that
location. The local staged graph now creates the same normalized role
assignment, rejects attempts to assign a second role to the same contact/location
pair with Shopify's current `LIMIT_REACHED` userError for the single-role
assignment surface,
and resolves nested
`CompanyContactRoleAssignment.companyContact` / `.companyLocation` fields from
the current normalized contact/location records so later contact or location
updates are reflected in downstream assignment reads. Generic Admin
`node(id:)` / `nodes(ids:)` dispatch now resolves staged or captured
`CompanyContactRoleAssignment` IDs through the same assignment serializer and
`CompanyAddress` IDs from effective company-location billing/shipping address
payloads.

HAR-620 tightens B2B contact deletion and role-assignment guardrails from the
Business Customers implementation. Company contacts can carry local associated
order evidence in their normalized data (`ordersCount`,
`associatedOrdersCount`, `hasAssociatedOrders`, or an `orders` list). When that
marker indicates one or more orders, `companyContactDelete` returns
`FAILED_TO_DELETE` with Shopify's current "Cannot delete a company contact with
existing orders or draft orders." message and retains the contact. Successful
deletion continues to remove the contact from the company contact list, so
downstream `Company.mainContact` reads clear when the deleted contact was the
main contact.
Role-assignment mutation roots now reject missing or cross-company locations and
roles with `RESOURCE_NOT_FOUND` and Shopify's current company-location or
company-contact-role not-found messages instead of a generic `rolesToAssign`
error. The 2026-04 `b2b-contact-business-rule-preconditions` capture records the
duplicate role, foreign/missing role, foreign/missing location, successful main
contact delete, and completed B2B order-history delete rejection branches as
strict replayable parity evidence.

HAR-762 extends those role-assignment guardrails to revoke-role mutation roots.
`companyContactRevokeRole` validates the parent contact before looking at the
assignment and returns `INVALID_INPUT` with
`detail: "contact_does_not_match_company"` when the assignment exists on a
different contact. `companyContactRevokeRoles` rejects empty
`roleAssignmentIds` unless `revokeAll` is true, validates the parent contact,
and reports per-index ownership errors while still revoking valid IDs.
`companyLocationRevokeRoles` validates the parent location and reports
per-index `RESOURCE_NOT_FOUND` errors for assignments outside that location.
Focused runtime tests cover these branches and the no-cross-scope-mutation
invariant; no new passive parity scenarios were added without fresh capture
evidence.

HAR-754 aligns bulk B2B resolver `userErrors.field` paths with Shopify's
string-indexed list paths. Bulk company/contact/location deletes, role
assignment/revoke roots, and location staff assignment/removal roots report
failed entries at paths such as `["companyContactIds", "1"]` or
`["rolesToAssign", "0", "companyLocationId"]` while preserving top-level
single-ID field paths on the single-resource mutation surfaces. The 2026-04
capture records the Shopify quirk that `companyLocationAssignRoles` reports
missing contact/role entries at the indexed list item path (for example
`["rolesToAssign", "0"]`) rather than at a nested sub-field. Staff assignment
still does not synthesize a broader staff catalog, but missing staff-member and
staff-assignment IDs use Shopify's indexed user error paths and null payload
shape for the failed list-valued fields.

Company location tax settings are written by
`companyLocationTaxSettingsUpdate(...)` and can be read through the current
`CompanyLocation.taxSettings { taxRegistrationId taxExempt taxExemptions }`
shape. The proxy also preserves the earlier flat fields used by local tests for
compatibility with the staged record data, but `taxSettings` is the
live-captured 2026-04 readback shape. The flat mutation arguments follow
Shopify's validation boundary: invalid `TaxExemption` variable values are
rejected as top-level GraphQL `INVALID_VARIABLE` errors before the local
resolver runs, an update with no tax settings knob returns `NO_INPUT` at
`companyLocationId`, and explicit `taxExempt: null` returns `INVALID_INPUT` at
`taxExempt`.

Company location create/update input validation enforces HAR-612's
`billingSameAsShipping` and `billingAddress` guardrails before local staging:
`billingSameAsShipping: true` rejects a non-empty explicit `billingAddress`,
`billingSameAsShipping: false` rejects a missing or blank `billingAddress`, and
explicit `taxExempt: null` rejects with `INVALID_INPUT`. `companyCreate`
applies the same checks to its nested `companyLocation` input. The 2026-04
`b2b-billing-same-as-shipping-validation` capture gives strict executable
evidence for the live-reproduced payload userErrors: explicit billing while
`billingSameAsShipping` is true, and `taxExempt: null`, on `companyCreate` and
`companyLocationCreate`. That capture also records public-schema boundaries:
the active live target accepts the `billingSameAsShipping: false` / no billing
create branch, and does not expose these location fields on
`CompanyLocationUpdateInput`; those ticket-required guardrails are therefore
runtime-test-backed instead of parity-compared.

HAR-623 tightens B2B location/address lifecycle behavior. `companyLocationCreate`
now derives an omitted or blank location name from
`input.shippingAddress.address1` before falling back to the company name.
`companyLocationAssignAddress` rejects duplicate `addressTypes` entries before
staging an address, matching the captured `INVALID_INPUT` branch with a null
error field and `addresses: null`. `companyAddressDelete` detaches a deleted
address from every billing/shipping side that currently references it and clears
the local `billingSameAsShipping` flag when the deleted address was the shared
anchor. `companyLocationDelete` also removes contact role assignments that point
at the deleted location, so downstream `CompanyContact.roleAssignments` reads no
longer expose assignments to a missing location.

The HAR-623 2026-04 capture records one public Admin API wrinkle:
`billingSameAsShipping: true` with a shipping address returns separate public
`billingAddress` and `shippingAddress` IDs, and `billingSameAsShipping` itself is
not selectable on `CompanyLocation` in that schema. The local runtime still
models the shared same-as-shipping anchor as a single address ID so the internal
flag invariant can be tested directly; the parity spec documents the public
readback difference for that single captured path while the focused runtime test
covers the local shared-anchor cascade.

`companyContactSendWelcomeEmail` remains unsupported. It is an outbound side
effect rather than durable B2B state, so runtime passthrough remains the
unknown/unsupported escape hatch until a faithful no-send model exists.

### Validation and exclusions

Local guardrails cover the captured no-side-effect branches for blank company
names and unknown company/contact/location IDs. These return resolver-level
`userErrors` without appending commit-log work.

The local implementation intentionally models durable lifecycle state rather
than every Shopify-side integration. Customer and staff member references are
stored by ID for downstream B2B reads, but the proxy does not synthesize broader
customer or staff catalog side effects from B2B assignment mutations.
The HAR-446 live capture records that the current conformance token receives
`ACCESS_DENIED` for `staffMembers(first:)`, so staff assignment remains covered
by executable runtime tests instead of live staff-catalog parity. Generic Node
dispatch therefore keeps `CompanyLocationStaffMemberAssignment` and
`CompanyLocationCatalog` unsupported until staff/catalog behavior has
conformance-backed local modeling.

## Historical and developer notes

### Evidence

- Live capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/b2b/b2b-company-roots-read.json`
- Live lifecycle capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/b2b/b2b-company-create-lifecycle.json`
- Live contact/main/delete lifecycle capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/b2b/b2b-company-contact-main-delete.json`
- Live contact/location assignment and tax settings capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/b2b/b2b-contact-location-assignments-tax.json`
- Safe mutation validation capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/b2b/b2b-company-mutation-validation.json`
- Strict read parity scenario:
  `config/parity-specs/b2b/b2b-company-roots-read.json`
- Lifecycle parity scenario:
  `config/parity-specs/b2b/b2b-company-create-lifecycle.json`
- Contact/main/delete lifecycle parity scenario:
  `config/parity-specs/b2b/b2b-company-contact-main-delete.json`
- Contact/location assignment and tax settings parity scenario:
  `config/parity-specs/b2b/b2b-contact-location-assignments-tax.json`
- Contact business-rule preconditions capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/b2b/b2b-contact-business-rule-preconditions.json`
- Contact business-rule preconditions parity scenario:
  `config/parity-specs/b2b/b2b-contact-business-rule-preconditions.json`
- Location/address management capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/b2b/b2b-location-address-management.json`
- Location/address management parity scenario:
  `config/parity-specs/b2b/b2b-location-address-management.json`
- Lifecycle runtime coverage:
- Root inventory:
  `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`

The captured store had two companies, two company locations, per-company
system contact roles, one company contact, and safe unknown-ID null branches for
company/contact/role/location detail roots.

HAR-404 refreshed read-fidelity evidence without broadening mutation support:
runtime tests now cover B2B connection `edges`, stable local cursors,
`first`/`after` windows, nested contact-role/location connections, and
live-hybrid preservation of upstream access errors and forwarded auth headers.

The checked-in validation capture records safe unknown-ID branches for
`companyUpdate`, `companyLocationUpdate`, and `companyContactUpdate`. Shopify
returned `RESOURCE_NOT_FOUND` userErrors without mutating the store. These
guardrails now back local validation behavior for the promoted lifecycle model.

The HAR-363 live lifecycle capture used API `2026-04` against
`harry-test-heelo.myshopify.com`. `corepack pnpm conformance:probe` reported a
valid app token and the store accepted `companyCreate`; the recorder deleted the
disposable company with `companyDelete(id:)` after capturing the immediate
downstream read.

HAR-445 extended that 2026-04 evidence with a disposable company/customer
lifecycle capture. The new scenario records `companyUpdate`,
`companyContactCreate`, `companyAssignCustomerAsContact`,
`companyAssignMainContact`, `companyRevokeMainContact`, `companiesDelete`,
`companyDelete`, and post-delete `company(id:)` / `companies(query:)` empty
reads. The capture showed that contact creation materializes customer
references, revoking the main contact returns `Company.mainContact: null`, and
`companiesCount` does not accept a `query` argument in 2026-04.

HAR-625 adds local free-text guardrails for supported B2B mutations before any
staged state is written. Company and company-location `name` values are
HTML-stripped before blank/length checks and local staging; `name` values longer
than 255 characters fail with `TOO_LONG`. Company-contact `title` values longer
than 255 characters fail with `TOO_LONG`, and title/notes-style fields with
markup fail with `CONTAINS_HTML_TAGS`. Company and company-location `note`
inputs use Shopify's `notes` user-error field label and fail above 5000
characters. The 2026-04 `b2b-string-validation` parity capture on
`harry-test-heelo.myshopify.com` now gives executable strict evidence for the
live-reproduced length branches: `companyCreate` long name, `companyCreate`
long note, and `companyLocationCreate` long name. The same capture intentionally
keeps probe-only responses for current live mismatches: Shopify accepted HTML in
company notes/contact titles, accepted a 300-character contact title, and
reported only `TOO_LONG` for HTML-plus-too-long notes. Those internal-source
HTML/title branches remain covered by runtime tests rather than a misleading
parity spec.

HAR-608 adds local `externalId` guardrails for company and company-location
create/update mutations. The proxy enforces Shopify's 64-character maximum,
rejects characters outside the captured `ExternalIdValidator` allow-list with
`INVALID`, and checks staged per-shop uniqueness before writing local B2B state.
The public Admin API's `BusinessCustomerUserError` exposes `field`, `message`,
and `code`; internal validator detail remains covered by runtime tests when
selected locally. The 2026-04 live capture for
`b2b-external-id-validation` shows duplicate company and company-location
external IDs returning Shopify's observable `TAKEN` code, so the proxy emits
`TAKEN` for normal duplicate externalId validation rather than the lower-level
DB-conflict enum names. Update mutations use the same checks while allowing the
current record to retain its own unchanged external ID. Executable parity specs
cover charset, too-long, duplicate-company, and duplicate-location branches.
