defmodule ShopifyDraftProxy.LiveHybridE2ETest do
  @moduledoc """
  End-to-end smoke against a real test store, driven from Elixir through
  the Gleam draft-proxy library. Stages 3 productCreate mutations
  in-process, verifies read-after-write and that staged synthetic IDs do
  not exist upstream, runs `/__meta/commit` (which on Erlang really hits
  Shopify via `:httpc`), then deletes the committed products in a
  teardown block so the test store stays clean.

  Why this exists: the Elixir wrapper is the only consumer that runs the
  Gleam port through real Erlang (vs. Node), so this is the canonical
  cross-target proof that productCreate stages locally with a synthetic
  GID, that `__meta/commit` actually replays through `:httpc` to
  Shopify, and that the resulting real GIDs round-trip cleanly. If this
  test passes but `tests/integration/gleam-interop.test.ts` fails (or
  vice versa), there is a target-specific regression in the Gleam port.

  Tagged `:live` and excluded by default in `test_helper.exs` so plain
  `mix test` stays offline. Canonical entry point:

      pnpm e2e:elixir-product-create-commit-smoke

  which runs `scripts/elixir-e2e-product-create-commit-smoke.ts`. That
  wrapper rebuilds the Erlang shipment, refreshes the conformance token
  via `scripts/shopify-conformance-auth.mts`, and forwards
  `SHOPIFY_CONFORMANCE_*` to `mix test --only live`.

  Manual invocation (token already in env):

      cd elixir_smoke && mix test --only live test/live_hybrid_e2e_test.exs
  """
  use ExUnit.Case, async: false

  @moduletag :live

  @sample_count 3

  setup_all do
    {:ok, _} = Application.ensure_all_started(:inets)
    {:ok, _} = Application.ensure_all_started(:ssl)

    store_domain = require_env!("SHOPIFY_CONFORMANCE_STORE_DOMAIN")
    admin_origin = require_env!("SHOPIFY_CONFORMANCE_ADMIN_ORIGIN")
    api_version = require_env!("SHOPIFY_CONFORMANCE_API_VERSION")
    access_token = require_env!("SHOPIFY_CONFORMANCE_ACCESS_TOKEN")

    auth_headers = %{"X-Shopify-Access-Token" => access_token}

    {:ok,
     store_domain: store_domain,
     admin_origin: admin_origin,
     api_version: api_version,
     access_token: access_token,
     auth_headers: auth_headers}
  end

  test "creates 3 products via proxy in live-hybrid mode and commits them upstream",
       %{
         store_domain: store_domain,
         admin_origin: admin_origin,
         api_version: api_version,
         access_token: access_token,
         auth_headers: auth_headers
       } do
    proxy =
      ShopifyDraftProxy.with_config(
        read_mode: :live_hybrid,
        shopify_admin_origin: admin_origin
      )

    config = ShopifyDraftProxy.config(proxy)
    assert config.body =~ ~s("readMode":"live-hybrid")
    assert config.body =~ ~s("shopifyAdminOrigin":"#{admin_origin}")

    stamp = System.system_time(:millisecond)
    titles = for i <- 1..@sample_count, do: "Draft Proxy Elixir E2E #{stamp} ##{i}"

    {staged_ids, proxy_after_creates} =
      Enum.reduce(titles, {[], proxy}, fn title, {ids, current_proxy} ->
        mutation =
          ~s|mutation { productCreate(product: { title: "#{title}", status: DRAFT }) { product { id title handle status createdAt updatedAt } userErrors { field message } } }|

        response =
          ShopifyDraftProxy.graphql(current_proxy, mutation,
            api_version: api_version,
            headers: auth_headers
          )

        assert %ShopifyDraftProxy.Response{status: 200, body: body, proxy: next_proxy} = response

        decoded = JSON.decode!(body)
        assert %{"data" => %{"productCreate" => payload}} = decoded
        assert payload["userErrors"] == [], "unexpected userErrors: #{inspect(payload["userErrors"])}"

        product = payload["product"]
        assert product, "productCreate returned null product: #{inspect(decoded)}"
        assert is_binary(product["id"])
        assert String.starts_with?(product["id"], "gid://shopify/Product/"),
               "bad id shape: #{product["id"]}"
        assert String.contains?(product["id"], "shopify-draft-proxy=synthetic"),
               "missing synthetic id marker: #{product["id"]}"
        assert product["title"] == title
        assert product["status"] == "DRAFT"
        assert is_binary(product["handle"]) and product["handle"] != ""
        assert is_binary(product["createdAt"]) and product["createdAt"] != ""
        assert is_binary(product["updatedAt"]) and product["updatedAt"] != ""

        IO.puts("staged #{product["id"]}  title=\"#{product["title"]}\"  handle=#{product["handle"]}")
        {ids ++ [product["id"]], next_proxy}
      end)

    assert length(staged_ids) == @sample_count

    proxy_after_reads =
      Enum.zip(staged_ids, titles)
      |> Enum.with_index()
      |> Enum.reduce(proxy_after_creates, fn {{id, title}, idx}, current_proxy ->
        read_query =
          ~s|query Read($id: ID!) { product(id: $id) { id title status handle } }|

        response =
          ShopifyDraftProxy.graphql(current_proxy, read_query,
            api_version: api_version,
            headers: auth_headers,
            variables_json: JSON.encode!(%{"id" => id})
          )

        assert %ShopifyDraftProxy.Response{status: 200, body: body, proxy: next_proxy} = response
        decoded = JSON.decode!(body)
        product = get_in(decoded, ["data", "product"])
        assert product, "proxy could not read staged product #{idx}: #{body}"
        assert product["id"] == id
        assert product["title"] == title
        next_proxy
      end)

    IO.puts("read-after-write OK for #{length(staged_ids)} staged products")

    Enum.each(staged_ids, fn id ->
      read_query = ~s|query { product(id: "#{id}") { id title } }|

      {status, body} =
        shopify_direct_graphql(admin_origin, api_version, access_token, read_query)

      assert status == 200, "shopify direct returned #{status}: #{body}"
      decoded = JSON.decode!(body)
      assert decoded["data"]["product"] == nil,
             "Shopify unexpectedly returned a product for staged id #{id}: #{body}"
    end)

    IO.puts("confirmed staged products are NOT in Shopify yet")

    log = ShopifyDraftProxy.log(proxy_after_reads)
    log_decoded = JSON.decode!(log.body)

    staged_entries =
      Enum.filter(log_decoded["entries"] || [], fn entry ->
        entry["status"] == "staged" and entry["operationName"] == "productCreate"
      end)

    assert length(staged_entries) == @sample_count
    IO.puts("__meta/log shows #{length(staged_entries)} staged productCreate entries")

    commit_response = ShopifyDraftProxy.commit(proxy_after_reads, auth_headers)
    assert %ShopifyDraftProxy.Response{status: 200, body: commit_body, proxy: post_commit_proxy} = commit_response

    commit_decoded = JSON.decode!(commit_body)

    IO.puts(
      "commit response: ok=#{inspect(commit_decoded["ok"])} stopIndex=#{inspect(commit_decoded["stopIndex"])} attempts=#{length(commit_decoded["attempts"] || [])}"
    )

    assert commit_decoded["ok"] == true, "commit not ok: #{commit_body}"
    assert commit_decoded["stopIndex"] == nil
    assert length(commit_decoded["attempts"]) == @sample_count

    committed_ids =
      commit_decoded["attempts"]
      |> Enum.with_index()
      |> Enum.map(fn {attempt, i} ->
        assert attempt["success"] == true,
               "commit attempt #{i} failed: #{inspect(attempt)}"
        assert attempt["operationName"] == "productCreate"
        assert attempt["status"] == "committed"
        assert is_integer(attempt["upstreamStatus"]) and
                 attempt["upstreamStatus"] in 200..299
        assert attempt["upstreamError"] == nil
        upstream_product = get_in(attempt, ["upstreamBody", "data", "productCreate", "product"])
        assert upstream_product, "attempt #{i} missing upstream product: #{inspect(attempt)}"
        assert is_binary(upstream_product["id"])
        assert String.starts_with?(upstream_product["id"], "gid://shopify/Product/")
        refute String.contains?(upstream_product["id"], "shopify-draft-proxy=synthetic"),
               "attempt #{i} got a synthetic id back from Shopify: #{upstream_product["id"]}"
        assert upstream_product["title"] == Enum.at(titles, i)

        ue =
          get_in(attempt, ["upstreamBody", "data", "productCreate", "userErrors"]) || []

        assert ue == [], "attempt #{i} upstream userErrors: #{inspect(ue)}"

        IO.puts(
          "committed[#{i}]  staged=replaced  real=#{upstream_product["id"]}  title=\"#{upstream_product["title"]}\""
        )

        upstream_product["id"]
      end)

    post_commit_log = ShopifyDraftProxy.log(post_commit_proxy)
    post_commit_log_decoded = JSON.decode!(post_commit_log.body)
    entries = post_commit_log_decoded["entries"] || []

    assert Enum.count(entries, &(&1["status"] == "staged")) == 0
    assert Enum.count(entries, &(&1["status"] == "committed")) == @sample_count

    Enum.zip(committed_ids, titles)
    |> Enum.with_index()
    |> Enum.each(fn {{real_id, title}, idx} ->
      read_query = ~s|query { product(id: "#{real_id}") { id title status handle } }|

      {status, body} =
        shopify_direct_graphql(admin_origin, api_version, access_token, read_query)

      assert status == 200, "shopify direct returned #{status}: #{body}"
      decoded = JSON.decode!(body)
      product = get_in(decoded, ["data", "product"])
      assert product, "Shopify did not return committed product #{real_id}: #{body}"
      assert product["id"] == real_id
      assert product["title"] == title, "shopify title mismatch on committed[#{idx}]: #{body}"
    end)

    IO.puts(
      "verified all #{length(committed_ids)} committed products exist in Shopify (#{store_domain})"
    )

    IO.puts(
      "\nE2E SUCCESS: created #{@sample_count} products via Elixir+Gleam proxy and committed them to #{store_domain}"
    )

    on_exit(fn ->
      :inets.start()
      :ssl.start()
      IO.puts("\ncleaning up #{length(committed_ids)} committed product(s) from #{store_domain}...")

      Enum.each(committed_ids, fn real_id ->
        delete_mutation =
          ~s|mutation { productDelete(input: { id: "#{real_id}" }) { deletedProductId userErrors { field message } } }|

        try do
          {status, body} =
            shopify_direct_graphql(admin_origin, api_version, access_token, delete_mutation)

          if status == 200 do
            decoded = JSON.decode!(body)
            ue = get_in(decoded, ["data", "productDelete", "userErrors"]) || []

            if ue == [] do
              IO.puts("deleted #{get_in(decoded, ["data", "productDelete", "deletedProductId"]) || real_id}")
            else
              IO.warn("cleanup userErrors for #{real_id}: #{inspect(ue)}")
            end
          else
            IO.warn("cleanup HTTP #{status} for #{real_id}: #{body}")
          end
        rescue
          e -> IO.warn("cleanup failed for #{real_id}: #{Exception.message(e)}")
        end
      end)
    end)
  end

  defp require_env!(name) do
    case System.get_env(name) do
      nil ->
        raise "Missing required env var #{name}. Run via scripts/elixir-e2e-product-create-commit-smoke.ts to set it from the conformance auth helper."

      "" ->
        raise "Empty env var #{name}."

      value ->
        value
    end
  end

  defp shopify_direct_graphql(admin_origin, api_version, access_token, query) do
    body = JSON.encode!(%{"query" => query})
    url = "#{admin_origin}/admin/api/#{api_version}/graphql.json"

    request = {
      String.to_charlist(url),
      [
        {~c"X-Shopify-Access-Token", String.to_charlist(access_token)},
        {~c"Accept", ~c"application/json"}
      ],
      ~c"application/json",
      body
    }

    case :httpc.request(:post, request, [], body_format: :binary) do
      {:ok, {{_http_version, status, _reason}, _headers, response_body}} ->
        {status, response_body}

      {:error, reason} ->
        flunk("httpc request failed: #{inspect(reason)}")
    end
  end
end
