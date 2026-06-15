from __future__ import annotations

import json

from shopify_draft_proxy import DraftProxy, create_draft_proxy, default_http_transport

STAGE_MUTATION = (
    'mutation { savedSearchCreate(input: { name: "Promo orders", '
    'query: "tag:promo", resourceType: ORDER }) '
    "{ savedSearch { id name } userErrors { field message } } }"
)


def _stage_one_mutation(proxy: DraftProxy) -> None:
    response = proxy.process_graphql_request({"query": STAGE_MUTATION})
    assert response["status"] == 200


def test_custom_transport_runs_in_python_and_observes_the_commit_replay() -> None:
    captured: list[dict] = []

    def transport(request: dict) -> dict:
        captured.append(request)
        return {
            "status": 200,
            "headers": {"content-type": "application/json"},
            "body": json.dumps(
                {
                    "data": {
                        "savedSearchCreate": {
                            "savedSearch": {"id": "gid://shopify/SavedSearch/55"},
                            "userErrors": [],
                        }
                    }
                }
            ),
        }

    proxy = create_draft_proxy(
        shopify_admin_origin="https://example.myshopify.com",
        transport=transport,
    )
    _stage_one_mutation(proxy)

    result = proxy.commit(headers={"authorization": "Bearer test"})

    assert result["ok"] is True
    # The replay ran through our Python callable, exactly once for the one
    # staged mutation, against the configured origin.
    assert len(captured) == 1
    replay = captured[0]
    assert replay["method"] == "POST"
    assert replay["url"].startswith("https://example.myshopify.com/admin/api/")
    assert replay["url"].endswith("/graphql.json")
    assert replay["headers"]["authorization"] == "Bearer test"
    # Hop-by-hop headers are stripped by the shared Rust prep before we see it.
    assert "host" not in {name.lower() for name in replay["headers"]}
    assert "savedSearchCreate" in replay["body"]


def test_default_http_transport_translates_request_and_response_shapes() -> None:
    # The default transport is a plain callable; exercise its request/response
    # contract directly via a loopback HTTP server (no network egress).
    import http.server
    import threading

    received: dict = {}

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_POST(self) -> None:  # noqa: N802 - stdlib naming
            length = int(self.headers.get("content-length", "0"))
            received["body"] = self.rfile.read(length).decode("utf-8")
            received["auth"] = self.headers.get("authorization")
            payload = json.dumps({"data": {"ok": True}}).encode("utf-8")
            self.send_response(200)
            self.send_header("content-type", "application/json")
            self.send_header("content-length", str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)

        def log_message(self, *args: object) -> None:
            pass

    server = http.server.HTTPServer(("127.0.0.1", 0), Handler)
    host, port = server.server_address
    thread = threading.Thread(target=server.handle_request, daemon=True)
    thread.start()

    try:
        response = default_http_transport(
            {
                "method": "POST",
                "url": f"http://{host}:{port}/admin/api/2025-01/graphql.json",
                "headers": {"authorization": "Bearer secret", "content-type": "application/json"},
                "body": json.dumps({"query": "{ shop { name } }"}),
            }
        )
    finally:
        thread.join(timeout=5)
        server.server_close()

    assert response["status"] == 200
    assert json.loads(response["body"]) == {"data": {"ok": True}}
    assert received["auth"] == "Bearer secret"
    assert json.loads(received["body"]) == {"query": "{ shop { name } }"}


def test_commit_without_staged_mutations_does_not_invoke_transport() -> None:
    calls: list[dict] = []

    proxy = create_draft_proxy(transport=lambda request: calls.append(request) or {"status": 200, "body": "{}"})

    result = proxy.commit(headers={"authorization": "Bearer test"})

    assert result["ok"] is True
    assert calls == []
