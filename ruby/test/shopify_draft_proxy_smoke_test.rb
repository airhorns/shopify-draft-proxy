# frozen_string_literal: true

require "json"
require "minitest/autorun"
require "socket"
require "thread"

require "shopify_draft_proxy"

class LocalUpstream
  attr_reader :origin, :requests

  def initialize
    @requests = Queue.new
    @server = TCPServer.new("127.0.0.1", 0)
    @origin = "http://127.0.0.1:#{@server.addr[1]}"
    @thread = Thread.new { accept_loop }
  end

  def close
    @server.close
    @thread.join(1)
  end

  private

  def accept_loop
    loop do
      socket = @server.accept
      Thread.new(socket) { |client| handle(client) }
    end
  rescue IOError
    nil
  end

  def handle(socket)
    request_line = socket.gets&.strip
    headers = {}
    while (line = socket.gets)
      stripped = line.strip
      break if stripped.empty?

      key, value = stripped.split(":", 2)
      headers[key.downcase] = value.strip if key && value
    end
    body = socket.read(headers.fetch("content-length", "0").to_i)
    @requests << { request_line: request_line, headers: headers, body: body }
    payload = {
      data: {
        savedSearchCreate: {
          savedSearch: {
            id: "gid://shopify/SavedSearch/987654321",
            legacyResourceId: "987654321",
          },
          userErrors: [],
        },
      },
    }
    response = JSON.generate(payload)
    socket.write("HTTP/1.1 200 OK\r\n")
    socket.write("content-type: application/json\r\n")
    socket.write("content-length: #{response.bytesize}\r\n")
    socket.write("connection: close\r\n")
    socket.write("\r\n")
    socket.write(response)
  ensure
    socket.close
  end
end

class ShopifyDraftProxySmokeTest < Minitest::Test
  def test_answers_health_and_round_trips_state
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
    )

    health = proxy.process_request(method: "GET", path: "/__meta/health")
    assert_equal 200, health.status
    assert_equal true, health.body.fetch("ok")

    dump = proxy.dump_state(created_at: "2026-05-24T00:00:00.000Z")
    assert_equal "shopify-draft-proxy-rust-state/v1", dump.fetch("schema")

    restored = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      state: dump,
    )
    assert_kind_of Hash, restored.get_state
  ensure
    proxy&.dispose
    restored&.dispose
  end

  def test_stages_saved_search_reads_back_and_commits_with_auth_headers
    upstream = LocalUpstream.new
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: upstream.origin,
    )

    create = proxy.process_graphql_request(
      {
        query: <<~GRAPHQL,
          mutation {
            savedSearchCreate(input: { name: "Promo orders", query: "tag:promo", resourceType: ORDER }) {
              savedSearch { id name query resourceType }
              userErrors { field message }
            }
          }
        GRAPHQL
      },
    )
    assert_equal 200, create.status
    payload = create.body.fetch("data").fetch("savedSearchCreate")
    assert_equal [], payload.fetch("userErrors")
    assert_equal "Promo orders", payload.fetch("savedSearch").fetch("name")
    assert_includes payload.fetch("savedSearch").fetch("id"), "shopify-draft-proxy=synthetic"

    read = proxy.process_graphql_request(
      { query: '{ orderSavedSearches(query: "Promo") { nodes { id name } } }' },
    )
    nodes = read.body.fetch("data").fetch("orderSavedSearches").fetch("nodes")
    assert_equal ["Promo orders"], nodes.map { |node| node.fetch("name") }

    log = proxy.get_log
    assert_equal "staged", log.fetch("entries").first.fetch("status")

    commit = proxy.commit(headers: { "authorization" => "Bearer test-token" })
    assert_equal true, commit.fetch("ok")
    assert_equal 1, commit.fetch("committed")

    request = upstream.requests.pop
    assert_includes request.fetch(:request_line), "/admin/api/2025-01/graphql.json"
    assert_equal "Bearer test-token", request.fetch(:headers).fetch("authorization")
    assert_includes request.fetch(:body), "savedSearchCreate"
  ensure
    proxy&.dispose
    upstream&.close
  end
end
