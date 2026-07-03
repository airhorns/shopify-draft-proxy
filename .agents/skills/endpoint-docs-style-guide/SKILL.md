---
name: endpoint-docs-style-guide
description: Use when creating, reviewing, or auditing `docs/endpoints/*.md` for Shopify Admin GraphQL endpoint groups in shopify-draft-proxy. Covers endpoint documentation structure, consumer-facing fidelity standards, coverage/evidence language, unsupported-boundary wording, and where to move internal rationale or implementation history.
---

# Endpoint Docs Style Guide

Use this skill whenever you touch `docs/endpoints/*.md`. Endpoint docs are the
public internal reference for what the draft proxy currently does for a Shopify
Admin GraphQL area. They are not changelogs, ticket notes, or implementation
diaries.

## Audience

Write for someone deciding whether they can rely on a proxy endpoint today:

- app and test authors who need current supported behavior, known limits, and
  fidelity expectations
- agents implementing adjacent runtime behavior who need the current contract
- reviewers checking whether behavior claims are backed by executable evidence

Do not write for someone reconstructing how support evolved. If a sentence only
helps explain the order work landed in, remove it or move it to the right
internal note location.

## Required Shape

Each endpoint doc should follow this order unless the endpoint is small enough
that combining adjacent sections is clearer:

1. `# <Endpoint Group>`
2. A short scope paragraph naming the Shopify Admin GraphQL area and the main
   root families covered.
3. `## Current support and limitations`
4. `### Supported roots` or `### Implemented roots`
   - Use one heading consistently within the file.
   - List read roots and mutation roots separately when both exist.
   - Say `registry-only`, `unsupported`, or `tracked but unimplemented` for
     coverage-map roots that do not stage locally.
5. `### Local behavior`
   - Describe local state, read-after-write effects, validation, userErrors,
     GraphQL coercion behavior, no-data behavior, search/filter/sort/pagination,
     generic `node`/`nodes` behavior, and LiveHybrid snapshot differences that
     matter to consumers.
6. `### Boundaries`
   - State what remains unsupported, capture-only, validation-only, or out of
     scope. Explain the practical consequence for a caller.

For large endpoint groups, split `Local behavior` into domain-specific
subsections such as `Catalog reads`, `Lifecycle mutations`,
`Generic Node dispatch`, or `Commit replay`. Keep the top-level flow the same.
Do not add standalone proof, capture-summary, or command-list sections to
endpoint pages; keep proof details in workpads, PR descriptions, parity specs,
fixtures, and test files.

## Current-State Rules

- Document the state of the endpoint as it should be understood today.
- Prefer present tense: `stages`, `returns`, `hydrates`, `rejects`, `proxies`,
  `remains unsupported`.
- Be precise about support level. A validation guardrail, registry entry, local
  branch, or no-data resolver is not full operation support unless local
  lifecycle behavior and downstream read-after-write effects are modeled.
- Say whether supported mutations stage locally and retain original raw
  mutations for commit replay when that behavior applies.
- Distinguish runtime modes when behavior differs:
  - `snapshot`: local/snapshot state only, including Shopify-like empty/no-data
    responses
  - `LiveHybrid`: upstream reads plus local overlays or cassette-backed
    hydration where modeled
  - `passthrough` or unsupported handling only when it is a current boundary
- Avoid vague coverage claims such as `mostly supported`, `works`, or
  `handled`. Name the roots and the behavior surface.
- Do not turn implementation gaps into endpoint documentation as a substitute
  for fixing or tracking them. Use Linear/workpad follow-ups for future work.

## Prohibited Content

Endpoint docs must not contain support-history narratives:

- no `HAR-### added...`, `HAR-### reviewed...`, or ticket-review headings
- no `previously`, `formerly`, `now`, `later`, `initially`, or migration-order
  prose unless the word is describing current Shopify API semantics
- no implementation-port notes, dispatcher migration notes, or technology history
- no release-note style lists of what changed in a branch
- no claims that a partial branch is implemented support
- no unsupported-root reservations or planned-only parity placeholders

Ticket identifiers are allowed only when they are part of a stable artifact path
that already exists and is the clearest evidence reference. Do not put ticket
identifiers in headings or use them to explain why support exists.

## Internal Rationale Placement

Keep endpoint docs consumer-facing. Move rich internal context to the narrowest
durable location:

- `docs/hard-and-weird-notes.md`: Shopify quirks, surprising captures, rejected
  assumptions, and why a fidelity decision exists.
- `docs/helpers.md`: shared helper APIs, parser/serializer/search/connection
  utilities, and rules for adding or reusing helpers.
- `docs/architecture.md`: cross-cutting runtime architecture, request flow,
  state model, commit behavior, or API surfaces that affect multiple endpoint
  groups.
- `docs/parity-runner.md`: parity runner mechanics, cassette format, replay
  behavior, or comparison-contract guidance.
- Linear issues/workpads: temporary implementation plans, rejected-review
  context, future work, and out-of-scope remediation.

When moving rationale out of an endpoint doc, leave only the current consequence
in the endpoint doc. Example: write `LiveHybrid reads hydrate the existing
customer before staging this mutation` in the endpoint doc, and move the
debugging story that led to that shape to `docs/hard-and-weird-notes.md` if it
is still useful.

## Audit Checklist

Use this checklist when auditing an existing endpoint doc:

- [ ] The title and scope paragraph identify the Admin API area without history.
- [ ] Supported, unsupported, registry-only, and validation-only roots are
      clearly separated.
- [ ] Supported mutations are described as local staging, not runtime Shopify
      writes.
- [ ] Read-after-write, no-data, search/filter/sort/pagination, generic Node,
      and commit/meta behavior are covered when relevant.
- [ ] Unsupported boundaries explain caller-visible behavior and do not promise
      future work.
- [ ] Standalone proof, capture-summary, and command-list sections are absent.
- [ ] Proxy-generated, snapshot, runtime-test, or hand-authored artifacts are not
      described as captured Shopify parity evidence; endpoint claims separate
      live capture proof from proxy-only runtime regression tests.
- [ ] Ticket-review headings, support-history prose, and migration notes
      are removed or moved to the correct internal note location.
- [ ] The doc does not claim support beyond local lifecycle behavior and
      downstream read-after-write behavior.
