# Conformance Parity

Use this skill when adding, updating, or reviewing Shopify Admin GraphQL conformance scenarios, parity specs, or captured fixture comparisons.

## Scenario Requirements

- Preserve the project mission: this is a Shopify Admin GraphQL digital twin / draft proxy, not a generic mock server.
- Prefer product and directly related sub-resource fidelity before broad operation coverage.
- A captured scenario is high-assurance only when it has:
  - a concrete `proxyRequest` with a local request document,
  - a `comparison` object with `mode: "strict-json"`,
  - explicit comparison `targets`, and
  - path-scoped `allowedDifferences` with clear `reason` text.
- Do not add or preserve in-between scenario states. A scenario is either executable high-assurance comparison or explicitly not yet implemented.

## Allowed Differences

- Use `matcher` only for real nondeterminism that still proves the value has the right shape, such as Shopify GIDs, timestamps, or numeric throttle metadata.
- Use `ignore: true` only when the local proxy has a known, fixable parity gap and implementing the real Shopify behavior is proving too difficult for the current change.
- Every `ignore: true` rule must also set `regrettable: true`.
- Do not use `regrettable: true` to normalize avoidable drift. Prefer improving the proxy behavior until the strict comparison passes.

## Validation

- Run `corepack pnpm conformance:check` after changing scenario registries, operation registry entries, generated conformance docs, parity specs, or fixture references.
- Run `corepack pnpm conformance:parity` after changing parity specs, comparator behavior, proxy request documents, fixture variables, or local parity execution.
- Add or update tests for every supported operation or comparison rule you touch.
