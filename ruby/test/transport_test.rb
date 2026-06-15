# frozen_string_literal: true

require "json"
require "socket"
require "minitest/autorun"

require "shopify_draft_proxy"

class ShopifyDraftProxyTransportTest < Minitest::Test
  STAGE_MUTATION = <<~GRAPHQL
    mutation {
      savedSearchCreate(input: { name: "Promo orders", query: "tag:promo", resourceType: ORDER }) {
        savedSearch { id name }
        userErrors { field message }
      }
    }
  GRAPHQL

  def test_custom_transport_runs_in_ruby_and_observes_the_commit_replay
    captured = []
    transport = lambda do |request|
      captured << request
      {
        "status" => 200,
        "headers" => { "content-type" => "application/json" },
        "body" => JSON.generate(
          "data" => {
            "savedSearchCreate" => {
              "savedSearch" => { "id" => "gid://shopify/SavedSearch/55" },
              "userErrors" => [],
            },
          },
        ),
      }
    end

    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://example.myshopify.com",
      transport: transport,
    )
    assert_equal 200, proxy.process_graphql_request({ query: STAGE_MUTATION }).status

    result = proxy.commit(headers: { "authorization" => "Bearer test" })
    assert_equal true, result.fetch("ok")

    # The replay ran through our Ruby callable, once for the one staged mutation.
    assert_equal 1, captured.length
    replay = captured.first
    assert_equal "POST", replay.fetch("method")
    assert_match %r{\Ahttps://example\.myshopify\.com/admin/api/.+/graphql\.json\z}, replay.fetch("url")
    assert_equal "Bearer test", replay.fetch("headers").fetch("authorization")
    # Hop-by-hop headers are stripped by the shared Rust prep before Ruby sees it.
    refute_includes replay.fetch("headers").keys.map(&:downcase), "host"
    assert_includes replay.fetch("body"), "savedSearchCreate"
  ensure
    proxy&.dispose
  end

  def test_default_net_http_transport_translates_request_and_response_shapes
    received = {}
    server = TCPServer.new("127.0.0.1", 0)
    port = server.addr[1]

    thread = Thread.new do
      client = server.accept
      request_line = client.gets
      headers = {}
      while (line = client.gets) && line != "\r\n"
        name, value = line.split(":", 2)
        headers[name.strip.downcase] = value.strip
      end
      body = client.read(headers.fetch("content-length").to_i)
      received[:request_line] = request_line.strip
      received[:auth] = headers["authorization"]
      received[:body] = body

      payload = JSON.generate("data" => { "ok" => true })
      client.write("HTTP/1.1 200 OK\r\n")
      client.write("content-type: application/json\r\n")
      client.write("content-length: #{payload.bytesize}\r\n")
      client.write("\r\n")
      client.write(payload)
      client.close
    end

    response = ShopifyDraftProxy::Transports::DEFAULT.call(
      "method" => "POST",
      "url" => "http://127.0.0.1:#{port}/admin/api/2025-01/graphql.json",
      "headers" => { "authorization" => "Bearer secret", "content-type" => "application/json" },
      "body" => JSON.generate("query" => "{ shop { name } }"),
    )

    thread.join(5)

    assert_equal 200, response.fetch("status")
    assert_equal({ "data" => { "ok" => true } }, JSON.parse(response.fetch("body")))
    assert_equal "POST /admin/api/2025-01/graphql.json HTTP/1.1", received.fetch(:request_line)
    assert_equal "Bearer secret", received.fetch(:auth)
    assert_equal({ "query" => "{ shop { name } }" }, JSON.parse(received.fetch(:body)))
  ensure
    server&.close
  end

  def test_commit_without_staged_mutations_does_not_invoke_transport
    calls = []
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://example.myshopify.com",
      transport: ->(request) { calls << request; { "status" => 200, "body" => "{}" } },
    )

    result = proxy.commit(headers: { "authorization" => "Bearer test" })
    assert_equal true, result.fetch("ok")
    assert_empty calls
  ensure
    proxy&.dispose
  end
end
