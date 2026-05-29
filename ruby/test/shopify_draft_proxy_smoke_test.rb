# frozen_string_literal: true

require "json"
require "minitest/autorun"

require "shopify_draft_proxy"

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

  def test_stages_saved_search_and_reads_back_without_a_server_process
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
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
  ensure
    proxy&.dispose
  end

  def test_multiple_instances_keep_staged_state_independent
    first = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
    )
    second = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
    )

    first.process_graphql_request(
      {
        query: <<~GRAPHQL,
          mutation {
            savedSearchCreate(input: { name: "First proxy", query: "tag:first", resourceType: ORDER }) {
              savedSearch { id name query resourceType }
              userErrors { field message }
            }
          }
        GRAPHQL
      },
    )

    first_names = saved_search_names(first, "First")
    second_names = saved_search_names(second, "First")
    assert_equal ["First proxy"], first_names
    assert_equal [], second_names
    assert_equal 1, first.get_log.fetch("entries").length
    assert_equal 0, second.get_log.fetch("entries").length
  ensure
    first&.dispose
    second&.dispose
  end

  def test_dump_restore_preserves_state_without_linking_instances
    source = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
    )
    source.process_graphql_request(
      {
        query: <<~GRAPHQL,
          mutation {
            savedSearchCreate(input: { name: "Restored search", query: "tag:restored", resourceType: ORDER }) {
              savedSearch { id name query resourceType }
              userErrors { field message }
            }
          }
        GRAPHQL
      },
    )
    dump = source.dump_state(created_at: "2026-05-24T00:00:00.000Z")

    restored = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      state: dump,
    )
    restored.process_graphql_request(
      {
        query: <<~GRAPHQL,
          mutation {
            savedSearchCreate(input: { name: "Restored only", query: "tag:restored-only", resourceType: ORDER }) {
              savedSearch { id name query resourceType }
              userErrors { field message }
            }
          }
        GRAPHQL
      },
    )

    assert_equal ["Restored search"], saved_search_names(source, "Restored")
    assert_equal ["Restored search", "Restored only"], saved_search_names(restored, "Restored")
    assert_equal 1, source.get_log.fetch("entries").length
    assert_equal 2, restored.get_log.fetch("entries").length
  ensure
    source&.dispose
    restored&.dispose
  end

  private

  def saved_search_names(proxy, query)
    read = proxy.process_graphql_request(
      { query: "{ orderSavedSearches(query: #{JSON.generate(query)}) { nodes { id name } } }" },
    )
    read.body.fetch("data").fetch("orderSavedSearches").fetch("nodes").map { |node| node.fetch("name") }
  end
end
