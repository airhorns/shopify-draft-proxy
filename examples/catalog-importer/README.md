# Catalog importer example

A worked example of using `shopify-draft-proxy` the way a real embedder would.

It contains a small, ordinary Shopify app — [`CatalogImporter`](lib/catalog_importer.rb),
which seeds a store's product catalog from a data feed — built on the **official
`shopify_api` gem** (`ShopifyAPI::Clients::Graphql::Admin`). The importer knows
nothing about the draft proxy. Its [test suite](test/catalog_importer_test.rb)
then runs it end to end against an in-process draft proxy by intercepting the
official client's real HTTP requests.

## Why this exists

The proxy's own smoke tests call `process_graphql_request` directly with
hand-written GraphQL strings. That exercises the runtime, but it is _not_ how an
app talks to Shopify. A real app builds requests through a client library that
does its own query/variable serialization, header handling, and response
parsing. This example closes that gap:

```
CatalogImporter
   │  client.query(query:, variables:)
   ▼
ShopifyAPI::Clients::Graphql::Admin   ← official gem, real serialization
   │  POST https://<shop>/admin/api/2025-01/graphql.json
   ▼
Net::HTTP  ──intercepted by WebMock──►  proxy.process_request(...)
                                            │  in-process Rust runtime
                                            ▼
                                        staged state + Shopify-shaped response

                            ... later, on proxy.commit(...) ...

proxy.commit  ──►  Rust core  ──►  Ruby transport (Net::HTTP)  ──► upstream
                   replays each      releases the GVL during IO   (captured
                   staged mutation   so Ruby instrumentation /     by WebMock
                                     WebMock observes it           in tests)
```

WebMock intercepts at the `Net::HTTP` boundary, so the official client does all
of its real work — only the wire is swapped for the in-process proxy. If the
proxy got a response envelope, header, or field shape subtly wrong, the real
client would choke on it here. The commit replay rides the _same_ boundary
because its outbound HTTP also runs in Ruby (see [Findings](#findings-surfaced-by-running-this-end-to-end)).

## What it exercises

- **Read-after-write** — create products, then read them back by id and via a
  `products(query:)` read, seeing staged writes.
- **`userErrors` handling** — a blank title round-trips to the twin, which
  returns a domain `userError` the importer surfaces as a structured error.
- **GraphQL validation errors** — an invalid `status` enum surfaces as a
  top-level error at HTTP 200, which the app must check for itself.
- **Tag enrichment** — a follow-up `tagsAdd` merges into a product's tags.
- **Saved searches** — creating a `PRODUCT` saved search.
- **Instance isolation** — two proxies stage state independently.
- **State serialization** — dump the draft buffer, serialize it to a JSON
  string, drop the instance, then rehydrate a fresh proxy from the parsed
  string (simulating a checkpoint to Redis/disk between job runs). The test
  asserts the serialized buffer carries the staged products and mutation log,
  that the rehydrated proxy reads every product back faithfully, and that it is
  still committable.
- **Commit replay** — staging then `commit`, asserting the proxy replays each
  staged mutation upstream. Because the replay's HTTP now runs in Ruby, WebMock
  captures it in-process — the same boundary the inbound client uses.
- **Pluggable transport** — supplying a custom Ruby `transport:` callable to
  `ShopifyDraftProxy.create`, used here to record a span per replay (the seam an
  embedder would hook for tracing, retries, or a pooled connection).

## Findings surfaced by running this end to end

Driving the _real_ client through the twin turned up three things worth
recording. The tests pin the current behavior so a future change flips them
loudly rather than silently:

- **`products(query:)` filtering is not evaluated.** The twin returns every
  staged product regardless of the search query, so `search("vendor:Northwind")`
  comes back with all products, not just the Northwind ones. An importer that
  relies on server-side filtering would be surprised.
- **The log's top-level `operationName` is always `nil`.** It mirrors the
  request body's explicit `operationName` field, which the official client does
  not send. The parsed operation _is_ available under each log entry's
  `interpreted` object (`interpreted.primaryRootField`), which is what consumers
  should read.
- **`commit` performs its upstream HTTP in Ruby, releasing the GVL.** The
  commit replay (and any live-hybrid passthrough) runs through a pluggable
  _transport_ — a Ruby callable that does the actual request. The default is a
  plain `Net::HTTP` round-trip, so the GVL is released during socket IO and
  other Ruby threads keep running. Because the IO is Ruby's, Ruby-level
  instrumentation sees it too: the commit-replay test captures the upstream with
  WebMock _in-process_ (no separate OS process), and the transport test swaps in
  a custom callable. Earlier this work was done in Rust (reqwest) while holding
  the GVL, which is why the replay used to need an out-of-process capture server.

## What the twin does _not_ model (yet)

While building this, the example was kept to the operations the runtime actually
supports. As of this writing the twin does **not** model `productSet`,
`fileCreate`/media, `collectionCreate`, `publications`/`publishablePublish`,
product variant bulk operations, or product-level metafield read-back. Product
core fields (`title`, `handle`, `descriptionHtml`, `vendor`, `productType`,
`status`, `tags`), `tagsAdd`, and saved searches are modeled and read back
faithfully. Unsupported mutations are run with `unsupported_mutation_mode:
"reject"` in tests so an accidental dependency on an unmodeled operation fails
loudly.

## Running it

From this directory, with the gem's native extension built (see the repository
root and `ruby/`):

```sh
bundle install
bundle exec rake test     # the end-to-end suite
bundle exec ruby bin/import   # a runnable demonstration that prints staged state
```

The example depends on the in-repo gem via a relative path
(`gem "shopify-draft-proxy", path: "../../ruby"`), so it always runs against the
current source tree.
