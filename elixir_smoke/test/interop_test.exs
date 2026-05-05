defmodule ShopifyDraftProxy.InteropTest do
  use ExUnit.Case, async: true

  alias :shopify_draft_proxy@proxy@draft_proxy, as: DraftProxy

  test "Elixir wrapper exposes config, GraphQL product lifecycle, meta, reset, dump/restore, and commit reports" do
    proxy = ShopifyDraftProxy.new()

    config = ShopifyDraftProxy.config(proxy)
    assert %ShopifyDraftProxy.Response{status: 200, body: config_body} = config
    assert config_body =~ ~s("readMode":"snapshot")
    assert config_body =~ ~s("unsupportedMutationMode":"passthrough")

    create =
      ShopifyDraftProxy.graphql(proxy, ~s|
        mutation {
          productCreate(product: { title: "Elixir Wrapper Hat" }) {
            product {
              id
              title
              handle
              status
              variants(first: 1) {
                nodes {
                  id
                  title
                  inventoryItem { id tracked requiresShipping }
                }
              }
            }
            userErrors { field message }
          }
        }
      |)

    assert %ShopifyDraftProxy.Response{status: 200, body: create_body, proxy: created_proxy} =
             create

    assert create_body =~ ~s("productCreate")
    assert create_body =~ ~s("title":"Elixir Wrapper Hat")
    assert create_body =~ ~s("handle":"elixir-wrapper-hat")
    assert create_body =~ ~s("userErrors":[])
    [_, product_id] = Regex.run(~r/"id":"(gid:\/\/shopify\/Product\/[^"]+)"/, create_body)

    assert String.contains?(product_id, "shopify-draft-proxy=synthetic"),
           "ShopifyDraftProxy.new/0 must attach the default operation registry so " <>
             "supported mutations like productCreate are staged locally with a synthetic " <>
             "GID instead of being passed through to Shopify. Got: #{product_id}"

    read =
      ShopifyDraftProxy.graphql(
        created_proxy,
        ~s|query { product(id: "#{product_id}") { id title handle status variants(first: 1) { nodes { id title } } } }|
      )

    assert %ShopifyDraftProxy.Response{status: 200, body: read_body} = read
    assert read_body =~ ~s("product":{"id":"#{product_id}")
    assert read_body =~ ~s("title":"Elixir Wrapper Hat")

    state = ShopifyDraftProxy.state(created_proxy)
    assert state.body =~ "Elixir Wrapper Hat"

    log = ShopifyDraftProxy.log(created_proxy)
    assert log.body =~ "productCreate"

    dump = ShopifyDraftProxy.dump_state(created_proxy, "2026-04-30T00:00:00.000Z")
    assert dump =~ ~s("schema":"shopify-draft-proxy/state-dump")
    assert {:ok, restored_proxy} = ShopifyDraftProxy.restore_state(ShopifyDraftProxy.new(), dump)
    assert ShopifyDraftProxy.log(restored_proxy).body =~ "productCreate"

    fake_send = fn _request ->
      {:ok,
       {:http_outcome, 200,
        ~s({"data":{"productCreate":{"product":{"id":"gid://shopify/Product/999"},"userErrors":[]}}}),
        []}}
    end

    commit =
      ShopifyDraftProxy.commit_with(
        created_proxy,
        "https://shop.example",
        %{},
        fake_send
      )

    assert %ShopifyDraftProxy.CommitReport{ok: true, stop_index: nil, attempt_count: 1} =
             commit

    reset = ShopifyDraftProxy.reset(created_proxy)
    assert reset.status == 200
    assert ShopifyDraftProxy.log(reset.proxy).body == ~s({"entries":[]})

    empty_commit = ShopifyDraftProxy.commit(ShopifyDraftProxy.new())
    assert empty_commit.status == 200
    assert empty_commit.body =~ ~s("attempts":[])

    assert {:error, reason} = ShopifyDraftProxy.restore_state(proxy, "not json")
    assert is_tuple(reason)
  end

  test "phase 0 hello/0 is callable from elixir and returns the expected marker" do
    assert :shopify_draft_proxy.hello() ==
             "shopify_draft_proxy gleam port: phase 0"
  end

  test "request/5 handles GET /__meta/health and returns the documented envelope" do
    response = ShopifyDraftProxy.request(ShopifyDraftProxy.new(), "GET", "/__meta/health")

    assert %ShopifyDraftProxy.Response{status: 200, body: body, headers: []} = response
    assert body =~ ~s("ok":true)
    assert body =~ ~s("shopify-draft-proxy is running")
  end

  test "request/5 returns the next wrapper state so callers can thread it" do
    reset = ShopifyDraftProxy.request(ShopifyDraftProxy.new(), "POST", "/__meta/reset")
    assert %ShopifyDraftProxy.Response{status: 200, proxy: next_proxy} = reset

    assert %ShopifyDraftProxy.Response{status: 200} =
             ShopifyDraftProxy.request(next_proxy, "GET", "/__meta/health")
  end

  test "with_config accepts keyword options and returns an isolated wrapper proxy" do
    proxy =
      ShopifyDraftProxy.with_config(
        read_mode: :live_hybrid,
        unsupported_mutation_mode: :reject,
        shopify_admin_origin: "https://my-shop.myshopify.com"
      )

    config = ShopifyDraftProxy.config(proxy)
    assert config.body =~ ~s("readMode":"live-hybrid")
    assert config.body =~ ~s("unsupportedMutationMode":"reject")
    assert config.body =~ ~s("shopifyAdminOrigin":"https://my-shop.myshopify.com")
  end

  test "new/0 and with_config/1 attach the default operation registry" do
    for proxy <- [
          ShopifyDraftProxy.new(),
          ShopifyDraftProxy.with_config(read_mode: :snapshot)
        ] do
      registry = elem(proxy.raw, 4)

      assert is_list(registry) and length(registry) >= 60,
             "Elixir wrapper must attach the vendored operation registry; otherwise " <>
               "supported mutations resolve to Passthrough and leak to Shopify in " <>
               "live-hybrid mode (violates AGENTS.md non-negotiable #3). Got registry: " <>
               inspect(registry)
    end
  end

  test "default_graphql_path/1 builds the documented Admin path" do
    assert DraftProxy.default_graphql_path("2025-01") ==
             "/admin/api/2025-01/graphql.json"
  end

  test "graphql/3 convenience dispatches a POST to the Admin GraphQL path" do
    response =
      ShopifyDraftProxy.graphql(
        ShopifyDraftProxy.new(),
        saved_search_create_query("id name")
      )

    assert %ShopifyDraftProxy.Response{status: 200, body: body} = response
    assert body =~ "savedSearchCreate"
    assert body =~ "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic"
  end

  test "reset/1 clears mutation log and rewinds synthetic identity counter" do
    created =
      ShopifyDraftProxy.graphql(
        ShopifyDraftProxy.new(),
        saved_search_create_query("id")
      )

    assert ShopifyDraftProxy.log(created.proxy).body =~ "savedSearchCreate"

    reset = ShopifyDraftProxy.reset(created.proxy)
    assert ShopifyDraftProxy.log(reset.proxy).body == ~s({"entries":[]})

    refreshed =
      ShopifyDraftProxy.graphql(
        reset.proxy,
        saved_search_create_query("id")
      )

    assert refreshed.body =~ "gid://shopify/SavedSearch/1?shopify-draft-proxy=synthetic"
  end

  test "commit/2 returns the empty-log commit envelope" do
    commit = ShopifyDraftProxy.commit(ShopifyDraftProxy.new())

    assert %ShopifyDraftProxy.Response{status: 200, body: body} = commit
    assert body =~ ~s("ok":true)
    assert body =~ ~s("stopIndex":null)
    assert body =~ ~s("attempts":[])
  end

  test "commit_with/4 lets Elixir consumers inject a fake HTTP send" do
    created =
      ShopifyDraftProxy.graphql(
        ShopifyDraftProxy.new(),
        saved_search_create_query("id")
      )

    upstream_body =
      ~s({"data":{"savedSearchCreate":{"savedSearch":{"id":"gid://shopify/SavedSearch/12345"},"userErrors":[]}}})

    fake_send = fn _request -> {:ok, {:http_outcome, 200, upstream_body, []}} end
    report = ShopifyDraftProxy.commit_with(created.proxy, "https://shop.example", %{}, fake_send)

    assert %ShopifyDraftProxy.CommitReport{ok: true, stop_index: nil, attempt_count: 1} =
             report
  end

  test "commit_with/4 surfaces transport errors from the injected send" do
    created =
      ShopifyDraftProxy.graphql(
        ShopifyDraftProxy.new(),
        saved_search_create_query("id")
      )

    fake_send = fn _request -> {:error, {:commit_transport_error, "boom"}} end
    report = ShopifyDraftProxy.commit_with(created.proxy, "https://shop.example", %{}, fake_send)

    assert %ShopifyDraftProxy.CommitReport{ok: false, stop_index: 0, attempt_count: 1} =
             report
  end

  test "dump_state + restore_state round-trips synthetic identity counters" do
    created =
      ShopifyDraftProxy.graphql(
        ShopifyDraftProxy.new(),
        saved_search_create_query("id")
      )

    dump_json = ShopifyDraftProxy.dump_state(created.proxy, "2026-04-29T12:00:00.000Z")

    assert dump_json =~ ~s("schema":"shopify-draft-proxy/state-dump")
    assert dump_json =~ ~s("createdAt":"2026-04-29T12:00:00.000Z")
    assert dump_json =~ "savedSearchCreate"

    {:ok, restored} = ShopifyDraftProxy.restore_state(ShopifyDraftProxy.new(), dump_json)

    next =
      ShopifyDraftProxy.graphql(
        restored,
        saved_search_create_query("id")
      )

    # After restore, the next mint reuses the dump's counter. The first mutation
    # advanced the counter past 2 internally, so the next mint is /3.
    assert next.body =~ "gid://shopify/SavedSearch/3?shopify-draft-proxy=synthetic"
  end

  test "restore_state returns an error tuple on bad JSON" do
    {:error, reason} = ShopifyDraftProxy.restore_state(ShopifyDraftProxy.new(), "not json")
    assert is_tuple(reason)
    assert elem(reason, 0) == :malformed_dump_json
  end

  test "config/1 serializes the runtime config envelope" do
    response = ShopifyDraftProxy.config(ShopifyDraftProxy.new())

    assert response.body =~ ~s("readMode":"snapshot")
    assert response.body =~ ~s("port":4000)
    assert response.body =~ ~s("shopifyAdminOrigin":"https://shopify.com")
  end

  test "state/1 returns base + staged state envelope" do
    response = ShopifyDraftProxy.state(ShopifyDraftProxy.new())

    assert response.body =~ "baseState"
    assert response.body =~ "stagedState"
  end

  test "with_default_registry/1 attaches the vendored operation registry" do
    proxy = DraftProxy.new() |> DraftProxy.with_default_registry()
    registry = elem(proxy, 4)
    assert is_list(registry)
    assert length(registry) >= 60
  end

  defp saved_search_create_query(selection) do
    ~s|mutation { savedSearchCreate(input: { name: "Smoke", query: "tag:vip", resourceType: ORDER }) { savedSearch { #{selection} } userErrors { field message } } }|
  end
end
