from __future__ import annotations

from shopify_draft_proxy import (
    DRAFT_PROXY_STATE_DUMP_SCHEMA,
    DraftProxy,
    create_draft_proxy,
)


def test_health_and_config_are_served_by_native_runtime() -> None:
    proxy = create_draft_proxy(read_mode="snapshot", shopify_admin_origin="https://shopify.com")

    health = proxy.process_request("GET", "/__meta/health")

    assert health["status"] == 200
    assert health["body"]["ok"] is True
    assert "shopify-draft-proxy" in health["body"]["message"]
    assert proxy.get_config()["runtime"]["readMode"] == "snapshot"


def test_multiple_instances_keep_staged_state_independent() -> None:
    first = DraftProxy(read_mode="snapshot")
    second = DraftProxy(read_mode="snapshot")

    create_response = first.process_graphql_request(
        {
            "query": 'mutation { savedSearchCreate(input: { name: "Promo orders", query: "tag:promo", resourceType: ORDER }) { savedSearch { id name query resourceType } userErrors { field message } } }'
        }
    )
    assert create_response["status"] == 200

    staged_read = first.process_graphql_request(
        {"query": '{ orderSavedSearches(query: "Promo") { nodes { id name } } }'}
    )
    empty_read = second.process_graphql_request(
        {"query": '{ orderSavedSearches(query: "Promo") { nodes { id name } } }'}
    )

    assert staged_read["body"]["data"]["orderSavedSearches"]["nodes"] == [
        {
            "id": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic",
            "name": "Promo orders",
        }
    ]
    assert empty_read["body"]["data"]["orderSavedSearches"]["nodes"] == []
    assert first.get_log()["entries"]
    assert second.get_log()["entries"] == []


def test_dump_and_restore_round_trip_between_instances() -> None:
    source = create_draft_proxy()
    source.process_graphql_request(
        {
            "query": 'mutation { savedSearchCreate(input: { name: "Promo products", query: "tag:promo", resourceType: PRODUCT }) { savedSearch { id name query resourceType } userErrors { field message } } }'
        }
    )

    dump = source.dump_state("2026-05-29T00:00:00.000Z")

    assert dump["schema"] == DRAFT_PROXY_STATE_DUMP_SCHEMA
    assert dump["createdAt"] == "2026-05-29T00:00:00.000Z"

    restored = create_draft_proxy(state=dump)
    restored_read = restored.process_graphql_request(
        {"query": '{ productSavedSearches(query: "Promo") { nodes { id name } } }'}
    )

    assert restored_read["body"]["data"]["productSavedSearches"]["nodes"] == [
        {
            "id": "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic",
            "name": "Promo products",
        }
    ]

    restored.reset()
    reset_read = restored.process_graphql_request(
        {"query": '{ productSavedSearches(query: "Promo") { nodes { id name } } }'}
    )
    assert reset_read["body"]["data"]["productSavedSearches"]["nodes"] == []
