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

## Historical and developer notes

### Evidence

- Live capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/b2b/b2b-company-roots-read.json`
- Live lifecycle capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/b2b/b2b-company-create-lifecycle.json`
- Safe mutation validation capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/b2b/b2b-company-mutation-validation.json`
- Strict read parity scenario:
  `config/parity-specs/b2b/b2b-company-roots-read.json`
- Lifecycle parity scenario:
  `config/parity-specs/b2b/b2b-company-create-lifecycle.json`
- Runtime coverage: `tests/integration/b2b-company-query-shapes.test.ts`
- Lifecycle runtime coverage:
  `tests/integration/b2b-company-lifecycle-flow.test.ts`
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
