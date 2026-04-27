# B2B Company, Contact, And Location Roots

## Supported reads

HAR-302 adds narrow snapshot/parity support for these B2B Admin GraphQL roots:

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

## Evidence

- Live capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/b2b-company-roots-read.json`
- Safe mutation validation capture:
  `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/b2b-company-mutation-validation.json`
- Strict parity scenario: `config/parity-specs/b2b-company-roots-read.json`
- Runtime coverage: `tests/integration/b2b-company-query-shapes.test.ts`
- Root inventory:
  `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-graphql-root-operation-introspection.json`

The captured store had two companies, two company locations, per-company
system contact roles, one company contact, and safe unknown-ID null branches for
company/contact/role/location detail roots.

## Mutation Boundaries

The B2B mutation roots are inventoried in `config/operation-registry.json` under
the `b2b` domain, but remain `implemented: false`. They must not be treated as
supported until local lifecycle behavior and downstream read-after-write effects
are modeled:

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
- side effects: `companyContactSendWelcomeEmail`

The registry entries are coverage blockers, not passthrough commitments.
Runtime support should only be promoted when the proxy can stage the mutation
locally without writing to Shopify and subsequent B2B reads observe the staged
state.

The checked-in validation capture records safe unknown-ID branches for
`companyUpdate`, `companyLocationUpdate`, and `companyContactUpdate`. Shopify
returned `RESOURCE_NOT_FOUND` userErrors without mutating the store. These
guardrails are evidence for future lifecycle work, not local mutation support.
