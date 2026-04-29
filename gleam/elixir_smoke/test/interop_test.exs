defmodule ShopifyDraftProxy.InteropTest do
  use ExUnit.Case, async: true

  test "phase 0 hello/0 is callable from elixir and returns the expected marker" do
    assert :shopify_draft_proxy.hello() ==
             "shopify_draft_proxy gleam port: phase 0"
  end
end
