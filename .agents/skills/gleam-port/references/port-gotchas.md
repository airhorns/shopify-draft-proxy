# Port gotchas

Distilled from passes 1–20 of the Gleam port. Each entry is something a
prior pass had to learn the slow way; reading them up front saves a test
cycle.

## Cross-target

- **Run both targets, every time.** Drift between Erlang and JavaScript
  is the most expensive bug class. CI runs both; do not push without
  local confirmation.
- **FFI requires both shims.** Add `<name>_ffi.erl` and `<name>_ffi.mjs`
  next to the `.gleam` module. The `@external` attributes name the
  Erlang module bare (`"crypto_ffi"`) and the JS file by relative path
  (`"./crypto_ffi.mjs"`). For test files, the JS path traverses up to
  reach `src/` (e.g. `"../../shopify_draft_proxy/crypto_ffi.mjs"`).

## Synthetic identity

- **`make_synthetic_gid` vs `make_proxy_synthetic_gid` are not
  interchangeable.** The first produces `gid://shopify/Type/N` (looks
  like a real upstream id); the second appends
  `?shopify-draft-proxy=synthetic`. Each domain follows what the TS
  handler does for that resource. Test fixtures must use the right
  form; otherwise look-by-id misses silently. Pass 19 (gift cards) uses
  the proxy-synthetic suffix; pass 20 (segments) does not.
- **Identity threading is uniform but counter-coupled.** Each gid mint
  advances `next_synthetic_id`. A handler that mints both a resource
  gid and a log-entry gid produces predictable id pairs (e.g.
  `Foo/1` then `MutationLogEntry/2`). Reordering mints inside a
  handler shifts downstream ids — tests will need updating in lockstep.
- **Failed mutations still advance the counter.** A failed
  `*Create` still mints a `MutationLogEntry` gid. Tests that assert
  specific id values across multi-mutation sequences must account for
  this.

## Reserved names and module aliasing

- **`test` is a Gleam keyword.** Rename record fields to `is_test`. The
  GraphQL response key stays `"test"` because the source builder names
  it explicitly.
- **Function parameter named `store: Store` collides with a module
  imported as `state/store`.** `store.list_effective(store)` parses as
  field access. Fix: import the function unqualified
  (`import .../state/store.{type Store, list_effective_x}`).
- **`types` module + a local `types` symbol.** Use `as types_mod` on
  import to disambiguate. `state/store.gleam` does this.

## State and store

- **Store mutators return a fresh `Store`.** No in-place mutation. Every
  call site threads the new store forward via the `MutationOutcome`
  envelope.
- **Store field types must be added on both `BaseState` and
  `StagedState`.** Existing constructors switch to `..base` / `..staged`
  spread to preserve unrelated fields.
- **Static defaults are not in the staged store, so they cannot be
  deleted.** A delete against a static-default id surfaces the same
  user error as a delete against unknown id. Matches TS.

## Decoding

- **`decode.optional_field("k", default, inner)`** returns `default`
  only when the key is *absent*. To accept `null` too, the inner
  decoder must be `decode.optional(...)`. The combination handles both
  shapes; using only `optional_field` will crash on explicit `null`.
- **`decode.recursive` works.** Use it for self-referential decoders
  (e.g. a `ResolvedValue` that nests). Closure is invoked lazily; no
  trampolining needed.
- **`decode.one_of` for sum-shaped JSON.** Order branches carefully:
  on Erlang `false` is `0` for some primitive checks, so `bool` must
  come before `int` in the union. See `parse_request_body` in
  `draft_proxy.gleam`.

## Validation and arguments

- **AST validation and resolved-arg execution must stay split.** Only
  the AST distinguishes "argument omitted" from "literal null" from
  "unbound variable". Each maps to a distinct GraphQL `extensions.code`
  (`missingRequiredArguments` / `argumentLiteralsIncompatible` /
  `INVALID_VARIABLE`). Use `mutation_helpers.validate_required_field_arguments`
  for validation, `dict.get` on the resolved arg dict for execution.
- **`dict_has_key` distinguishes "key present with null" from "key
  absent".** Some mutation inputs (e.g. `recipientAttributes` on
  `giftCardUpdate`) treat null as "clear" and absent as "preserve".
  Mirror TS `Object.prototype.hasOwnProperty.call` exactly.

## GraphQL projection

- **Inline-fragment + FragmentSpread is per-domain boilerplate.** The
  generic `project_graphql_value` walks plain `Field` selections only.
  Domains with `__typename`-discriminated unions (gift cards, apps
  pricing, webhook endpoints) carry inline `walk_typed_selections`
  helpers locally. A future pass should consider lifting this; until
  then, copy the pattern from the closest existing domain.
- **Connections-as-source-values vs `serialize_connection`.** Top-level
  connection roots use `paginate_connection_items` + `serialize_connection`
  directly. Connections nested inside a parent record's projection use
  a `SourceValue` shaped like a connection (`{__typename, edges, nodes,
  pageInfo, totalCount}`) — the parent's `project_graphql_value` walk
  will descend into it.
- **Pagination on nested connections is not honored.** Most nested
  connection projections emit a fixed page (no `first` / `after`
  filtering). Acceptable for current parity; flagged in pass 16/17/18
  logs. If a test exercises pagination on a nested connection, the
  source builder needs lifting.

## No-regex policy

- **Don't pull in `gleam_regexp`** for one-off predicate sets. The
  project depends only on `gleam_stdlib` + `gleam_json`. Hand-roll
  string predicates with `string.starts_with` / `string.trim_start` /
  `string.length` / `int.parse`. Pass 20 ports 5 TS regexes to ~80 LOC
  of straight-line predicate functions and keeps the dependency
  footprint clean. Revisit only if a pass needs ≥10+ regex patterns or
  backtracking.

## Empty / null shape

- **Match Shopify's empty/no-data behaviour.** When a resource is
  missing, return `null` for single-resource roots, an empty connection
  for connection roots. Inherited from `AGENTS.md`.
- **`Option(String)` semantics for sort tiebreaks.** TS's
  `(left.x ?? '').localeCompare(...)` collapses null and empty into the
  same bucket. Match with `option.unwrap("", _)` + `string.compare`.

## Mutation log

- **No log entry for top-level error mutations.** When AST validation
  fires, the per-field handler short-circuits before
  `record_mutation_log_entry` runs. TS records "failed" entries; the
  Gleam port currently does not (per Pass 13 risks). Symmetric gap
  with saved-searches' "failed" entries.
- **`__meta/state` does not yet serialize most resource slices.** Only
  saved searches landed full meta-state coverage. Adding a slice is
  small; do it when a consumer needs offline introspection.

## Ergonomics

- **Dispatcher signatures are starting to feel heavy** (5+ parameters,
  domain mutation handlers take 7). Pass 6 flagged this. If the next
  pass needs to add another parameter (operationName, request id,
  fragments cache), consider lifting a `Dispatch` context record.
- **Test setup for `Selection` values is tedious.** The cleanest way to
  build a real `Selection` in a test is to parse a query string
  (`first_root_field("{ root { ... } }")`) and pull the root field. No
  AST literal syntax is provided; this is acceptable.
