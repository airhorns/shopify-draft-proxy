---
name: shopify-area-issue-generation
description: |
  Generate Linear issue batches for adding high-fidelity support for a new
  Shopify Admin GraphQL API area in shopify-draft-proxy. Use when a ticket asks
  to create issues for a Shopify area such as customers, orders, draft orders,
  inventory, collections, fulfillments, discounts, or another Admin API resource
  family.
---

# Shopify Area Issue Generation

Use this skill to turn a seed Linear issue such as "run the issue generation
skill for the Shopify customers area" into a researched set of Backlog Linear
issues that future agents can execute.

The output is not a generic worklist. It must preserve the project mission:
`shopify-draft-proxy` is a high-fidelity Shopify Admin GraphQL digital twin.
Generated issues should drive conformance-backed local staging, snapshot/read
fidelity, and read-after-write behavior without sending supported mutations to
Shopify at runtime.

## Required Inputs

Start from a seed Linear issue that names:

- the Shopify Admin API area, for example `customers`, `orders`, `inventory`, or
  `discounts`
- any explicit scope limits from the requester
- the Linear team/project/state that generated issues should inherit

If the area name is ambiguous, infer the most likely Admin GraphQL resource
family from Shopify docs and record the assumption in the seed issue workpad.
Only stop if Linear auth is unavailable or the seed issue does not identify a
usable Shopify area.

## Research Workflow

1. Read the repo context:
   - `AGENTS.md`
   - `docs/original-intent.md`
   - `docs/architecture.md`
   - `docs/hard-and-weird-notes.md`
   - `config/operation-registry.json`
   - existing `config/parity-specs/*.json` for the target domain and adjacent
     domains
   - relevant `config/parity-requests/`, `fixtures/conformance/`, `pending/`,
     `scripts/capture-*`, and `tests/integration/*` files
2. Review prior Linear issues in the same project before drafting new ones.
   Search for the target area, adjacent roots, and operation names. Avoid
   duplicates; update the plan with any existing issue that already covers a
   slice.
3. Review Shopify Admin GraphQL documentation for the current/latest version
   unless the seed issue pins a version. Use primary Shopify docs or schema
   introspection, not third-party summaries.
4. Inventory the target area:
   - root queries, connections, counts, and search/sort/filter arguments
   - mutations and their payload/userErrors shapes
   - object fields, nested connections, money sets, timestamps, enum/status
     fields, media/files, metafields, tags, addresses, and ownership links
   - fields with privacy, safety, side-effect, or business-risk implications
     such as email, phone, addresses, payment, tax, inventory, fulfillment,
     notification, customer identity, access scopes, and external sends
   - downstream reads that should observe a staged mutation
   - unsupported roots that would currently proxy upstream
5. Compare Shopify's surface to the repo:
   - implemented registry entries vs missing operation roots
   - parity specs with `planned`, `captured`, and `ready-for-comparison` status
   - fixture coverage, pending blockers, and capture scripts
   - runtime tests that already prove local staging/read behavior
   - docs notes that mention known traps or credentials limitations

## Decomposition Rules

Create as many Linear issues as needed for future agents to make steady
progress, but do not create one issue per field mechanically. Group work by
coherent Shopify behavior and validation evidence.

Prefer issue slices like:

- area inventory and operation registry gaps
- catalog/count/search reads
- detail object graph reads
- one mutation family or lifecycle workflow
- validation/userErrors and GraphQL validation branches
- parity fixture capture and strict comparison promotion
- local staging plus downstream read-after-write visibility
- sensitive/side-effect-heavy operations that need explicit safety decisions
- shared parser or serializer work only when multiple generated issues depend
  on it

Keep dependencies explicit. Foundational issues should block follow-up issues
when later work depends on registry/scenario/capture groundwork.

## Issue Body Shape

Use the existing project style. Each generated issue should have this structure:

```md
## Context

<Why this slice matters for high-fidelity Shopify Admin GraphQL behavior.>

Relevant sources:

- Shopify docs: `<root/query/mutation/object names>` in Admin GraphQL `<version>`
- Local registry/specs/tests/docs: `<specific repo paths>`
- Existing Linear context: `<seed issue or related issue identifiers when useful>`

## Scope

- <Concrete operations, fields, scenarios, or workflows included.>
- <Explicit exclusions if the slice is intentionally narrow.>

## Acceptance Criteria

- [ ] <Fixture/parity/registry/test/doc expectation.>
- [ ] <Local staging or read-after-write expectation when applicable.>
- [ ] <Validation/userErrors/nullability/search/sort/pageInfo expectation.>
- [ ] <Safety expectation for side effects or unsupported passthrough.>

## Validation

- [ ] `corepack pnpm conformance:check`
- [ ] `corepack pnpm conformance:parity`
- [ ] `corepack pnpm typecheck`
- [ ] <Targeted integration/unit/conformance command for this slice.>
```

Do not put vague instructions such as "support customers" or "add more tests" in
an issue. Name the operation roots and the evidence the future agent must
produce.

## Linear Creation Workflow

Use the `linear` skill and `linear_graphql` for all Linear writes.

1. Resolve the seed issue and record:
   - `team.id`
   - `project.id`
   - `state` IDs, especially `Backlog`
   - current assignee or the `harrymees` user when the workflow requires it
2. Search for duplicates with the target operation/root names before creating
   each issue.
3. Draft the full issue list in the seed issue workpad first. Include titles,
   dependency notes, and a short reason each issue is distinct.
4. Run a self-review pass:
   - remove duplicates
   - merge issues that are too small
   - split issues that mix unrelated lifecycle phases
   - verify every issue has conformance or runtime validation
   - verify generated issues preserve the proxy mission and do not normalize
     unsupported upstream writes as acceptable behavior
5. Create issues in `Backlog`, in the same Linear project as the seed issue, and
   assigned to the required user for the workflow.
   - Prefer one `issueCreate` mutation per issue. Linear has returned opaque
     HTTP 400 errors for batched creation and long issue bodies; if that
     happens, shorten the body without dropping acceptance criteria and retry
     issues individually.
6. Relate every generated issue back to the seed issue with `related`.
7. Add `blocks` relations between generated issues when ordering matters.
   If issue B depends on issue A, create a relation where A blocks B.
8. Update the seed workpad with created issue identifiers, duplicate decisions,
   unresolved research gaps, and validation performed. Do not add separate
   summary comments unless the calling workflow explicitly requires them.
9. Verify created issue metadata and relations before handoff. Linear relation
   queries can be directional, so check the seed issue's outgoing relations and,
   when links appear missing, also check the generated issue's relation list.

Useful Linear GraphQL mutations:

```graphql
mutation CreateIssue($input: IssueCreateInput!) {
  issueCreate(input: $input) {
    success
    issue {
      id
      identifier
      url
    }
  }
}
```

```graphql
mutation RelateIssues($input: IssueRelationCreateInput!) {
  issueRelationCreate(input: $input) {
    success
    issueRelation {
      id
      type
    }
  }
}
```

For generated issues, the `IssueCreateInput` should normally include:

```json
{
  "teamId": "<seed team id>",
  "projectId": "<seed project id>",
  "stateId": "<Backlog state id>",
  "assigneeId": "<harrymees or required assignee id>",
  "title": "<specific issue title>",
  "description": "<issue body>"
}
```

## Shopify Fidelity Checklist

Before creating the final issue set, make sure the set covers the target area
from the perspective of realistic app behavior:

- read empty/no-data behavior for singular lookups and connections
- catalog pagination, `pageInfo`, cursors, counts, search, filtering, sort, and
  reverse semantics
- detail object graphs and directly related sub-resources
- create/update/delete or lifecycle mutations that should be staged locally
- immediate downstream reads after staged writes
- userErrors, GraphQL validation errors, access-scope blockers, and nullability
- fixture capture scripts and parity specs before broad runtime claims
- sensitive data and externally visible side effects
- meta API observability where staged mutation logs/state should expose the new
  area
- docs updates only when runtime architecture or project intent changes

## Safety Rules

- Do not instruct future agents to send a supported mutation upstream at
  runtime.
- Treat `expectedDifferences` as a last resort after modeling or fixture seeding
  has been exhausted.
- Do not create `.mjs` scripts; repo scripts must be TypeScript run with `tsx`
  or an equivalent TypeScript runner.
- Do not create a checked-in project-management worklist. Linear is the
  worklist; repo files should remain implementation, docs, fixtures, and tests.
- If live Shopify credentials are missing, generated issues should describe the
  exact capture blocker and still include local/snapshot/fixture paths where
  useful.
