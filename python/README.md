# shopify-draft-proxy for Python

This folder contains Python bindings for the Rust `shopify-draft-proxy` runtime.
The package is built as a native extension with maturin/PyO3 and depends on the
same Rust crate used by the HTTP server and TypeScript shim.

## Install locally

From the repository root:

```bash
uv pip install -e ./python --extra test
```

For local smoke tests:

```bash
corepack pnpm python:test
```

## Usage

```python
from shopify_draft_proxy import DraftProxy, create_draft_proxy

proxy = create_draft_proxy(read_mode="snapshot")

health = proxy.process_request("GET", "/__meta/health")
assert health["status"] == 200

create = proxy.process_graphql_request(
    {
        "query": 'mutation { savedSearchCreate(input: { name: "Promo orders", query: "tag:promo", resourceType: ORDER }) { savedSearch { id name } userErrors { field message } } }'
    }
)
assert create["status"] == 200

commit = proxy.commit(headers={"authorization": "Bearer test-token"})
assert commit["ok"] is True
proxy.dispose()
```

Each `DraftProxy` owns an independent native Rust `DraftProxy` instance. Multiple
objects in one Python process do not share staged state, mutation logs, or
synthetic ID counters.

## Transports (outbound HTTP)

The proxy's _outbound_ HTTP — the `commit` replay and any live-hybrid
passthrough reads — runs in Python, not Rust. The Rust core hands a Python
**transport** callable a request `dict` and expects a response `dict` back:

```python
# request  -> {"method", "url", "headers", "body"}
# response <- {"status", "headers", "body"}
```

Doing the IO in Python means the GIL is released during the socket wait (so
other threads keep running) and Python-level instrumentation — OpenTelemetry,
`responses`, `requests-mock` — observes the request like any other `urllib`
call.

The default transport (`shopify_draft_proxy.default_http_transport`) is a plain
`urllib` round-trip. Supply your own to add tracing, retries, or a pooled
connection:

```python
from shopify_draft_proxy import create_draft_proxy, default_http_transport

def traced(request):
    with tracer.start_as_current_span("shopify.commit") as span:
        span.set_attribute("http.url", request["url"])
        return default_http_transport(request)

proxy = create_draft_proxy(
    shopify_admin_origin="https://example.myshopify.com",
    transport=traced,
)
```

A transport is any callable taking the request `dict`. When omitted, the default
`urllib` transport is installed.

## Dump and restore

The Python API uses the Rust runtime's existing state dump schema:

```python
source = DraftProxy(read_mode="snapshot")
dump = source.dump_state("2026-05-29T00:00:00.000Z")

restored = DraftProxy(read_mode="snapshot", state=dump)
restored.restore_state(dump)
```

The dump is a normal Python dictionary and can be serialized with `json.dumps`.
Restore accepts the same dictionary shape returned by `dump_state`.

## API

- `create_draft_proxy(**kwargs)` creates a `DraftProxy`.
- `DraftProxy(...)` accepts `read_mode`, `shopify_admin_origin`, `port`,
  `snapshot_path`, `unsupported_mutation_mode`, optional `state`, and an
  optional `transport` callable (see [Transports](#transports-outbound-http)).
- `default_http_transport(request)` is the stdlib `urllib` transport used when
  no `transport` is supplied; call it directly to delegate from a wrapper.
- `process_request(method, path, body=None, headers=None)` returns
  `{ "status": int, "body": object, "headers": dict? }`.
- `process_graphql_request(body, api_version="2025-01", path=None, headers=None)`
  posts a Shopify Admin GraphQL request.
- `get_config()`, `get_log()`, and `get_state()` expose the meta snapshots.
- `dump_state(created_at=None)` returns the Rust state dump dictionary.
- `restore_state(dump)` restores a Rust state dump.
- `reset()` clears staged state and logs for that instance.
- `commit(headers=None)` replays staged mutations through the Rust commit path.
- `dispose()` and `origin()` are no-op compatibility helpers for parity with the
  other language package surfaces.
