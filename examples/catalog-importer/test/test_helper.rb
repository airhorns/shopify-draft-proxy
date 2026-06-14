# frozen_string_literal: true

require "logger" # shopify_api references ::Logger but does not require it (Ruby 3.4+).
require "json"

require "shopify_api"
require "shopify_draft_proxy"

require "minitest/autorun"
require "webmock/minitest"

require_relative "../lib/catalog_importer"

# Shared end-to-end harness. The important idea: the official Shopify client is
# configured exactly as it would be against a real shop, and its real HTTP
# request is intercepted at the network boundary and forwarded into an
# in-process draft proxy. The app under test never knows the difference.
module ProxyHarness
  SHOP = "catalog-importer-demo.myshopify.com"
  API_VERSION = "2025-01"
  ACCESS_TOKEN = "shpat_draft_proxy_example"

  GRAPHQL_URL = %r{\Ahttps://#{Regexp.escape(SHOP)}/admin/api/[^/]+/graphql\.json\z}

  # Where the proxy replays staged mutations on `commit`. Distinct from SHOP so
  # the inbound client stub and the commit-replay stub never collide.
  UPSTREAM_ORIGIN = "https://upstream.shopify.example"
  UPSTREAM_GRAPHQL_URL = %r{\Ahttps://upstream\.shopify\.example/admin/api/[^/]+/graphql\.json\z}

  class << self
    # The proxy that the (single) WebMock stub forwards to. Reading this
    # dynamically at request time — rather than closing over a specific proxy —
    # means re-bridging to a different instance just reassigns this, instead of
    # stacking a second stub that shadows the first.
    attr_accessor :current_proxy
  end

  module_function

  # Configure the official client once for the whole suite, as a private app.
  def setup_context!
    return if @context_ready

    ShopifyAPI::Context.setup(
      api_key: "draft-proxy-example-key",
      api_secret_key: "draft-proxy-example-secret",
      api_version: API_VERSION,
      host_name: "localhost",
      scope: ["read_products", "write_products"],
      is_embedded: false,
      is_private: true,
      logger: Logger.new(IO::NULL),
    )
    @context_ready = true
  end

  def client
    session = ShopifyAPI::Auth::Session.new(shop: SHOP, access_token: ACCESS_TOKEN)
    ShopifyAPI::Clients::Graphql::Admin.new(session: session)
  end

  # Route the client's real GraphQL POSTs into the given in-process proxy. Safe
  # to call repeatedly within a test to switch the active proxy.
  def bridge!(proxy)
    self.current_proxy = proxy
    WebMock.stub_request(:post, GRAPHQL_URL).to_return do |request|
      response = ProxyHarness.current_proxy.process_request(
        method: "POST",
        path: request.uri.path,
        headers: request.headers,
        body: request.body,
      )
      {
        status: response.status,
        headers: { "Content-Type" => "application/json" }.merge(response.headers || {}),
        body: JSON.generate(response.body),
      }
    end
  end

  # Capture the mutations the proxy replays upstream during `commit`. Because
  # the commit transport now performs its HTTP in Ruby (Net::HTTP), WebMock
  # intercepts the replays in-process — no separate capture server needed.
  # Returns the array that accumulates each captured replay request.
  def capture_upstream_replays!
    captured = []
    WebMock.stub_request(:post, UPSTREAM_GRAPHQL_URL).to_return do |request|
      captured << {
        "path" => request.uri.path,
        "token" => request.headers["X-Shopify-Access-Token"],
        "body" => JSON.parse(request.body),
      }
      {
        status: 200,
        headers: { "Content-Type" => "application/json" },
        body: JSON.generate("data" => {}),
      }
    end
    captured
  end
end
