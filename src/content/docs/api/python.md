---
title: Python Library
description: Python API reference for embedding the proxy runtime in-process.
---

The Python library embeds the Rust `DraftProxy` runtime in the current Python
process through a PyO3 native extension. It does not start the local HTTP server
and does not reimplement Shopify routing in Python.

Each `create_draft_proxy(...)` or `DraftProxy(...)` call owns an independent
proxy instance with its own staged state, mutation log, synthetic IDs, and
transport hooks.

## Install

The package name is `shopify-draft-proxy` and the import package is
`shopify_draft_proxy`.

For a released package:

```sh
python -m pip install shopify-draft-proxy
```

From this repository checkout, install the Python package in editable mode:

```sh
uv pip install -e ./python
```

Install the test extra when running the package smoke suite:

```sh
uv pip install -e ./python --extra test
corepack pnpm python:test
```

## Configuration and Auth

```python
from shopify_draft_proxy import create_draft_proxy

proxy = create_draft_proxy(
    read_mode="snapshot",
    unsupported_mutation_mode="reject",
    shopify_admin_origin="https://your-store.myshopify.com",
)
```

Common keyword arguments:

| Argument                    | Purpose                                                                  |
| --------------------------- | ------------------------------------------------------------------------ |
| `read_mode`                 | `"snapshot"`, `"live-hybrid"`, or `"passthrough"`. Defaults to snapshot. |
| `unsupported_mutation_mode` | `"passthrough"` or `"reject"`.                                           |
| `shopify_admin_origin`      | Upstream Shopify origin used for live reads and commit replay.           |
| `port`                      | Compatibility config value. The Python package does not listen on it.    |
| `snapshot_path`             | Optional snapshot file loaded into the runtime.                          |
| `state`                     | Optional state dump from `dump_state`.                                   |
| `transport`                 | Optional callable for upstream reads and commit replay.                  |

Auth headers are not stored on the proxy. Pass Shopify Admin auth headers on the
request that may reach Shopify, or on `commit` when intentionally replaying
staged writes upstream:

```python
import os

headers = {
    "x-shopify-access-token": os.environ["SHOPIFY_ADMIN_ACCESS_TOKEN"],
}

proxy.process_graphql_request(
    {"query": "{ shop { name } }"},
    headers=headers,
)

proxy.commit(headers=headers)
```

Supported mutations are staged locally during normal runtime. `commit` is the
explicit write-through boundary and replays the original staged mutation bodies
in order.

## Quickstart

This example runs fully in snapshot mode and does not require a live Shopify
token.

```python
from shopify_draft_proxy import create_draft_proxy

proxy = create_draft_proxy(
    read_mode="snapshot",
    unsupported_mutation_mode="reject",
    shopify_admin_origin="https://example.myshopify.com",
)

try:
    create = proxy.process_graphql_request(
        {
            "query": """
            mutation {
              savedSearchCreate(input: { name: "Promo orders", query: "tag:promo", resourceType: ORDER }) {
                savedSearch { id name query resourceType }
                userErrors { field message }
              }
            }
            """
        }
    )
    assert create["status"] == 200, create["body"]

    read = proxy.process_graphql_request(
        {"query": '{ orderSavedSearches(query: "Promo") { nodes { id name } } }'}
    )
    nodes = read["body"]["data"]["orderSavedSearches"]["nodes"]
    print(nodes)
    print(len(proxy.get_log()["entries"]))
finally:
    proxy.dispose()
```

`process_request` is available when a test needs exact route behavior:

```python
health = proxy.process_request("GET", "/__meta/health")
print(health["status"])
print(health["body"])
```

## Transport Hooks

Python performs the proxy's outbound HTTP through a transport callable. The
default transport uses `urllib`, and custom transports can add tracing,
responses/requests-mock integration, retries, or a shared connection pool.

```python
from shopify_draft_proxy import create_draft_proxy, default_http_transport


def transport(request: dict) -> dict:
    # request => {"method", "url", "headers", "body"}
    return default_http_transport(request)


proxy = create_draft_proxy(
    shopify_admin_origin="https://example.myshopify.com",
    transport=transport,
)
```

The transport must return `{ "status": int, "headers": dict, "body": str }`.

## Limitations

- The Python library follows the same operation coverage as the Rust runtime.
  Check the endpoint reference for supported lifecycle behavior before relying
  on a root.
- `origin()` returns `None` and `dispose()` is a no-op because the Python package
  does not spawn an HTTP server process.
- Unsupported mutations may still passthrough when
  `unsupported_mutation_mode="passthrough"` is configured.
- The repository source is the authoritative package reference when a registry
  release is not available yet.

## References

- [Python package source](https://github.com/airhorns/shopify-draft-proxy/tree/main/python)
- [Python package metadata](https://github.com/airhorns/shopify-draft-proxy/blob/main/python/pyproject.toml)
- [Python README](https://github.com/airhorns/shopify-draft-proxy/blob/main/python/README.md)
- [Python smoke tests](https://github.com/airhorns/shopify-draft-proxy/tree/main/python/tests)
