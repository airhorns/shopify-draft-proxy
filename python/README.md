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
  `snapshot_path`, `unsupported_mutation_mode`, and optional `state`.
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
