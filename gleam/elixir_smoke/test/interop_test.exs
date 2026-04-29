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
end
