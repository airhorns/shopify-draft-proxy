defmodule ShopifyDraftProxy.InteropTest do
  use ExUnit.Case, async: true

  alias :shopify_draft_proxy@proxy@draft_proxy, as: DraftProxy

  test "phase 0 hello/0 is callable from elixir and returns the expected marker" do
    assert :shopify_draft_proxy.hello() ==
             "shopify_draft_proxy gleam port: phase 0"
  end

  test "default config tuple shape matches what the README documents" do
    config = DraftProxy.default_config()

    assert {:config, :snapshot, 4000, "https://shopify.com", :none} = config
  end

  test "process_request handles GET /__meta/health and returns the documented envelope" do
    proxy = DraftProxy.new()

    request = {:request, "GET", "/__meta/health", %{}, ""}

    {response, _next_proxy} = DraftProxy.process_request(proxy, request)

    assert {:response, 200, json_tree, []} = response

    body = :gleam@json.to_string(json_tree)
    assert body =~ ~s("ok":true)
    assert body =~ ~s("shopify-draft-proxy is running")
  end

  test "process_request returns the next proxy state so callers can thread it" do
    proxy = DraftProxy.new()

    request = {:request, "POST", "/__meta/reset", %{}, ""}

    {{:response, 200, _body, _headers}, next_proxy} =
      DraftProxy.process_request(proxy, request)

    refute is_nil(next_proxy)

    {{:response, 200, _, _}, _} =
      DraftProxy.process_request(next_proxy, {:request, "GET", "/__meta/health", %{}, ""})
  end

  test "with_config accepts a custom Config tuple" do
    config =
      {:config, :live_hybrid, 4000, "https://my-shop.myshopify.com", :none}

    proxy = DraftProxy.with_config(config)

    {{:response, 200, _, _}, _} =
      DraftProxy.process_request(proxy, {:request, "GET", "/__meta/health", %{}, ""})
  end

  test "default_graphql_path/1 builds the documented Admin path" do
    assert DraftProxy.default_graphql_path("2025-01") ==
             "/admin/api/2025-01/graphql.json"
  end

  test "process_graphql_request convenience dispatches a POST to the Admin GraphQL path" do
    proxy = DraftProxy.new()
    options = DraftProxy.default_graphql_request_options()

    body =
      ~s|{"query":"mutation { savedSearchCreate(input: { name: \\"Smoke\\", query: \\"tag:vip\\", resourceType: ORDER }) { savedSearch { id name } userErrors { field message } } }"}|

    {{:response, 200, json_tree, _}, _next_proxy} =
      DraftProxy.process_graphql_request(proxy, body, options)

    response = :gleam@json.to_string(json_tree)
    assert response =~ "savedSearchCreate"
    assert response =~ "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic"
  end

  test "reset/1 clears mutation log and rewinds synthetic identity counter" do
    proxy = DraftProxy.new()
    options = DraftProxy.default_graphql_request_options()

    body =
      ~s|{"query":"mutation { savedSearchCreate(input: { name: \\"Smoke\\", query: \\"tag:vip\\", resourceType: ORDER }) { savedSearch { id } userErrors { field message } } }"}|

    {_, mutated} = DraftProxy.process_graphql_request(proxy, body, options)

    log_before = :gleam@json.to_string(DraftProxy.get_log_snapshot(mutated))
    assert log_before =~ "savedSearchCreate"

    cleared = DraftProxy.reset(mutated)
    log_after = :gleam@json.to_string(DraftProxy.get_log_snapshot(cleared))
    assert log_after == ~s({"entries":[]})

    {{:response, 200, json_tree, _}, _} =
      DraftProxy.process_graphql_request(cleared, body, options)

    refreshed = :gleam@json.to_string(json_tree)
    assert refreshed =~ "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic"
  end

  test "/__meta/commit returns the empty-log commit envelope" do
    proxy = DraftProxy.new()

    {{:response, 200, json_tree, _}, _} =
      DraftProxy.process_request(
        proxy,
        {:request, "POST", "/__meta/commit", %{}, ""}
      )

    body = :gleam@json.to_string(json_tree)
    assert body =~ ~s("ok":true)
    assert body =~ ~s("stopIndex":null)
    assert body =~ ~s("attempts":[])
  end

  test "dump_state + restore_state round-trips synthetic identity counters" do
    proxy = DraftProxy.new()
    options = DraftProxy.default_graphql_request_options()

    body =
      ~s|{"query":"mutation { savedSearchCreate(input: { name: \\"Smoke\\", query: \\"tag:vip\\", resourceType: ORDER }) { savedSearch { id } userErrors { field message } } }"}|

    {_, mutated} = DraftProxy.process_graphql_request(proxy, body, options)

    dump_json =
      :gleam@json.to_string(
        DraftProxy.dump_state(mutated, "2026-04-29T12:00:00.000Z")
      )

    assert dump_json =~ ~s("schema":"shopify-draft-proxy/state-dump")
    assert dump_json =~ ~s("createdAt":"2026-04-29T12:00:00.000Z")
    assert dump_json =~ "savedSearchCreate"

    {:ok, restored} = DraftProxy.restore_state(DraftProxy.new(), dump_json)

    {{:response, 200, json_tree, _}, _} =
      DraftProxy.process_graphql_request(restored, body, options)

    next = :gleam@json.to_string(json_tree)
    # After restore, the next mint reuses the dump's counter — the first mutation
    # advanced the counter past 2 internally, so the next mint is /3.
    assert next =~ "gid://shopify/SavedSearch/3?shopify-draft-proxy=synthetic"
  end

  test "restore_state returns an error tuple on bad JSON" do
    proxy = DraftProxy.new()
    {:error, reason} = DraftProxy.restore_state(proxy, "not json")
    assert is_tuple(reason)
    # First element of the error variant tuple is the constructor name
    assert elem(reason, 0) == :malformed_dump_json
  end

  test "get_config_snapshot serializes the runtime config envelope" do
    proxy = DraftProxy.new()
    json_str = :gleam@json.to_string(DraftProxy.get_config_snapshot(proxy))
    assert json_str =~ ~s("readMode":"snapshot")
    assert json_str =~ ~s("port":4000)
    assert json_str =~ ~s("shopifyAdminOrigin":"https://shopify.com")
  end

  test "get_state_snapshot returns base + staged state envelope" do
    proxy = DraftProxy.new()
    json_str = :gleam@json.to_string(DraftProxy.get_state_snapshot(proxy))
    assert json_str =~ "baseState"
    assert json_str =~ "stagedState"
  end
end
