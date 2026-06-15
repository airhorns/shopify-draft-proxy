---
title: 'B2B Company, Contact, And Location Roots'
description: 'Coverage notes and fidelity boundaries for B2B Company, Contact, And Location Roots.'
---

This endpoint group tracks Shopify Admin GraphQL Business Customers roots for
companies, company contacts, company locations, roles, staff assignments,
company-location tax settings, and B2B address behavior.

## Current support and limitations

### Implemented local roots

The Rust operation registry marks the B2B roots below as locally implemented:
they route through `LOCAL_DISPATCH_ROOTS` and are answered without runtime
Shopify writes. This is not a support claim for the whole B2B domain.

Implemented read roots:

- `company`
- `companyLocation`

Implemented mutation roots:

- `companyAssignCustomerAsContact`
- `companyCreate`
- `companyLocationTaxSettingsUpdate`
- `companyLocationUpdate`
- `companyUpdate`

Other B2B roots remain registry-known but unsupported until the proxy has
local lifecycle/read models and executable evidence for those roots.

### Local behavior

The Rust runtime keeps selected B2B behavior as local slices for ported parity
and runtime coverage. These slices stage only the modeled root and shape covered
by their checked-in request documents and tests; they do not promote the entire
B2B root family to supported status.

`companyCreate` and `companyUpdate` have a Rust-tail local slice for company
identity fields. That slice stages synthetic company records, preserves
read-after-write through `company(id:)` for staged IDs, validates blank and
overlong names, strips HTML from accepted names, validates `externalId`
character set, length, and duplicate values, rejects HTML/overlong notes, and
rejects `companyUpdate(input.customerSince)` without changing the staged
company. Successful staged mutations append replay-ready log entries with the
original raw GraphQL request.

`companyLocationTaxSettingsUpdate` has a local tax-settings slice for required
and nullable input handling, invalid `TaxExemption` enum coercion, assignment
and removal of exemptions, `taxExempt`, and downstream payload projection.
Validation failures return B2B `userErrors` and mark the log entry as failed
when no state is staged.

`companyLocationUpdate` has a buyer-experience-configuration slice for the
ported request family. It validates empty configuration, deposit without a
payment-terms template, and disabled deposit feature branches, then stages
`editableShippingAddress`, `checkoutToDraft`, `paymentTermsTemplate`, and
`deposit` for downstream `companyLocation(id:)` reads when the input is valid.

Fixture-backed read helpers cover stable B2B read shapes that the Rust port
still uses as evidence, including `company.customerSince`,
`CompanyContactRoleAssignment.companyContact`, and
`CompanyContactRoleAssignment.companyLocation`. These helpers are evidence for
the selected payloads, not a broad local B2B catalog implementation.

Older parity specs still describe rich B2B lifecycle behavior captured from
Shopify, including company/contact/location lifecycle, role-assignment cleanup,
address management, deletion blockers, bulk field paths, and staff-assignment
guardrails. Until the Rust registry and dispatcher expose those behaviors as
local staging roots, endpoint consumers should treat them as captured evidence
and porting targets rather than full current-domain support.

### Boundaries

- `companyContactSendWelcomeEmail` remains unsupported because it is an outbound
  customer-visible email side effect. The proxy has no no-send model for it.
- Staff assignment does not synthesize a staff catalog. Unknown staff and
  assignment IDs are covered by validation evidence, while authorized
  staff-catalog reads remain access-scope dependent.
- Validation-only B2B parity specs prove guardrail payloads and no-stage
  behavior for those inputs only. They do not make the corresponding mutation
  roots generally supported.
- Generic `node(id:)` / `nodes(ids:)` dispatch for B2B-only IDs is limited to
  fixture-backed or tail-helper evidence. Do not infer complete Node support for
  companies, contacts, locations, addresses, staff assignments, or catalogs.
- Unsupported B2B mutation handling must remain visible as passthrough or
  reject behavior according to runtime configuration unless a request matches a
  documented local runtime slice.

### Evidence

- Registry status: `src/operation_registry.rs`
- Runtime tail-helper coverage: `tests/graphql_routes.rs`
- Read and lifecycle parity specs: `config/parity-specs/b2b/*.json`
- Read and lifecycle fixtures: `fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/b2b/*.json` and `fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/b2b/*.json`
- Root inventory fixture: `fixtures/conformance/very-big-test-store.myshopify.com/2025-01/admin-platform/admin-graphql-root-operation-introspection.json`

### Validation

- `corepack pnpm lint`
- `corepack pnpm rust:test`
