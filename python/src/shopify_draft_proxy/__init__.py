"""Python bindings for the Shopify Admin GraphQL draft proxy Rust runtime."""

import urllib.error
import urllib.request

from ._native import DRAFT_PROXY_STATE_DUMP_SCHEMA, DraftProxy, DraftProxyCommitError, create_draft_proxy

__all__ = [
    "DRAFT_PROXY_STATE_DUMP_SCHEMA",
    "DraftProxy",
    "DraftProxyCommitError",
    "create_draft_proxy",
    "default_http_transport",
]


def default_http_transport(request):
    """The default transport: a plain ``urllib`` round-trip over the stdlib.

    Transports perform the proxy's *outbound* HTTP — the commit replay and any
    live-hybrid passthrough reads. The native runtime hands the transport a
    request ``dict`` ``{"method", "url", "headers", "body"}`` and expects a
    response ``dict`` ``{"status", "headers", "body"}`` back. Crucially this
    work happens in Python, so the GIL is released during socket IO and
    Python-level instrumentation (OpenTelemetry, responses, requests-mock, ...)
    observes the request.

    Provide your own callable via
    ``create_draft_proxy(transport=lambda request: ...)`` — for example to add
    tracing, retries, or route through a shared connection pool.
    """
    body = request.get("body")
    data = body.encode("utf-8") if body else None
    http_request = urllib.request.Request(
        request["url"],
        data=data,
        method=(request.get("method") or "POST").upper(),
    )
    for name, value in (request.get("headers") or {}).items():
        http_request.add_header(str(name), str(value))

    try:
        with urllib.request.urlopen(http_request) as response:
            status = response.status
            headers = {name: value for name, value in response.headers.items()}
            payload = response.read().decode("utf-8")
    except urllib.error.HTTPError as error:
        # An HTTP error status is still a valid upstream response — forward it
        # rather than surfacing a transport failure.
        status = error.code
        headers = {name: value for name, value in error.headers.items()}
        payload = error.read().decode("utf-8")

    return {"status": status, "headers": headers, "body": payload}
