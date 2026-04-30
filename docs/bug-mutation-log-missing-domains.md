# Bug: Four domain handlers do not record `MutationLogEntry`

## Summary

The mutation handlers in `gift_cards`, `localization`, `metafield_definitions`, and
`segments` stage their effects into the domain store and return synthetic GraphQL
responses, but **never call `store.record_mutation_log_entry/2`**. As a result, the
mutation log returned by `get_log_snapshot/1` stays empty even though the underlying
store has been mutated and synthetic IDs have been minted.

This breaks any consumer that uses the mutation log as the canonical signal for
"is the buffer non-empty?". In particular, the Shell client (Shopify's
`shell_core` Elixir broker) drives its draft-buffer banner off
`length(get_log_snapshot(store).entries)`. With these four domains, the merchant
fires a mutation, gets a synthetic-looking response back, but the banner never
appears — there is no way to discover the staged work, commit it, or discard it.

## Severity

**High for any draft-proxy consumer.** The synthetic response is observable in
the GraphQL reply, but the staged mutation is invisible to log-based UI/state
machines. A merchant who issues `giftCardCreate` ends up with a phantom record
that has no audit trail and no path back through `commit` / `reset` from a UI
perspective.

## Affected modules and call sites

All four expose `pub fn process_mutation(store, identity, _request_path, document, variables)`,
discard `request_path`, and call a private `handle_mutation_fields` whose
signature **does not include** `request_path` or `document`:

| Module                                                       | `process_mutation`         | `handle_mutation_fields`     |
| ------------------------------------------------------------ | -------------------------- | ---------------------------- |
| `src/shopify_draft_proxy/proxy/gift_cards.gleam`             | line 1208 (discards arg)   | line 1224                    |
| `src/shopify_draft_proxy/proxy/localization.gleam`           | (entrypoint pattern match) | line 749                     |
| `src/shopify_draft_proxy/proxy/metafield_definitions.gleam`  | (entrypoint pattern match) | line 163                     |
| `src/shopify_draft_proxy/proxy/segments.gleam`               | (entrypoint pattern match) | line 659                     |

Smoking gun in `gift_cards.gleam`:

```gleam
pub fn process_mutation(
  store: Store,
  identity: SyntheticIdentityRegistry,
  _request_path: String,        // <-- discarded
  document: String,
  variables: Dict(String, root_field.ResolvedValue),
) -> Result(MutationOutcome, GiftCardsError) {
  case root_field.get_root_fields(document) {
    Error(err) -> Error(ParseFailed(err))
    Ok(fields) -> {
      let fragments = get_document_fragments(document)
      Ok(handle_mutation_fields(store, identity, fields, fragments, variables))
      //                                          ^^^^^^^^^^^^^^^^^^^^
      //                                          missing request_path + document
    }
  }
}
```

The per-mutation handlers (`handle_gift_card_create`, etc.) then stage into the
domain-specific store via `store.stage_create_gift_card/2` (and similar) and
return `MutationFieldResult { ..., staged_resource_ids: [record.id] }` — but
they never construct or append a `MutationLogEntry`.

## What "correct" looks like

Domains that already record correctly are `apps`, `bulk_operations`, `functions`,
`marketing`, `saved_searches`, and `webhooks`. The reference implementation is
`src/shopify_draft_proxy/proxy/webhooks.gleam`:

1. `process_mutation` propagates `request_path` and `document` into
   `handle_mutation_fields` (line 747).
2. Each per-mutation handler builds a `MutationLogEntry` with `synthetic_identity`
   for the log id and timestamp, and the originating `request_path` / `document`,
   plus the `staged_resource_ids` it just minted, and a `store.EntryStatus`
   computed from `user_errors` (`Staged` if empty, `Failed` otherwise).
3. The handler then calls `store.record_mutation_log_entry(store_after, entry)`
   (lines 943, 1052, 1137 in `webhooks.gleam`) and threads the resulting store
   forward.

`apps.gleam` factors the same logic out into a small helper around line 1980
(`build_log_entry` / `record_mutation_log_entry`); either approach is fine, but
copy-pasting that helper into each broken module is the smallest change.

## Reproduction

Against an `agent-cli` or HTTP harness wrapping the Gleam port:

```bash
mutation { giftCardCreate(input: {initialValue: {amount: "5.00", currencyCode: CAD}}) { giftCard { id } userErrors { field message } } }
```

Observe:

- The response includes `id: "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic"`.
- `get_log_snapshot(proxy.store)` returns `%{entries: []}`.

Same pattern for:

- `shopLocaleEnable`, `shopLocaleUpdate`, `shopLocaleDisable`,
  `translationsRegister`, `translationsRemove` (localization)
- `standardMetafieldDefinitionEnable` (metafield_definitions)
- `segmentCreate`, `segmentUpdate`, `segmentDelete`,
  `customerSegmentMembersQueryCreate` (segments)

Compare against `webhookSubscriptionCreate` (works) — the synthetic response is
returned **and** `entries` contains a `MutationLogEntry`.

## Fix

For each of the four affected modules:

1. Change `process_mutation`'s `_request_path` to `request_path` and pass it +
   `document` through to `handle_mutation_fields`.
2. Update `handle_mutation_fields`'s signature to take `request_path: String`
   and `document: String` and pass them through to each per-mutation handler.
3. In each per-mutation handler (`handle_gift_card_create`, `handle_segment_create`,
   etc.):
   - After computing `staged_resource_ids` and the next store, mint a
     `MutationLogEntry` using `synthetic_identity.make_synthetic_gid` and
     `make_synthetic_timestamp` for `id`/`received_at`.
   - Set `path: request_path` and `document: document` (or whatever fields the
     existing `MutationLogEntry` constructor takes — see `apps.build_log_entry`
     around line 2006 for the canonical shape).
   - Set `status: store.Staged` when there are no user errors,
     `store.Failed` otherwise.
   - Call `store.record_mutation_log_entry(store_after, entry)` and thread the
     resulting store into the returned tuple.

`marketing.gleam:2366` shows a single-line variant (`#(store.record_mutation_log_entry(store, entry), identity)`)
which is fine if the surrounding handler already returns a `(store, identity)`
shape; otherwise mirror the explicit variant from `webhooks.gleam`.

## Verification

After the fix, the same reproduction mutations above must:

- Continue to return the same synthetic response.
- Cause `get_log_snapshot(proxy.store).entries` to grow by exactly one
  `MutationLogEntry` per dispatched root mutation field, with `staged_resource_ids`
  populated and `status: Staged`.
- Leave the existing query-after-mutation behavior intact (the staged-effects
  visibility once the buffer is non-empty).

A unit test should be added for at least one mutation per affected domain that
asserts both the synthetic response shape and the post-call log length.

## Downstream context (why this matters)

Shell's `Shell.Agent.DraftBuffer` GenServer (at `packages/shell_core/lib/shell/agent/draft_buffer.ex`
in the `shop/world` monorepo, shell zone) derives its `:empty` vs `:active`
stage from `length(entries)` of `get_log_snapshot/1`. The client renders a
"<N> staged operations ready to apply or discard" banner only when stage is
`:active`. With this bug, the four affected domains stage real changes into the
proxy store but the banner never appears, the `clear-draft-buffer` agent tool
reports "already empty", and the merchant has no way to surface or clean up the
phantom state without ending the session.
