# frozen_string_literal: true

require "json"
require "minitest/autorun"

require "shopify_draft_proxy"

# The native extension marks itself Ractor-safe (rb_ext_ractor_safe in init), which
# lets a DraftProxy be constructed and driven from a non-main Ractor. Each instance
# owns its staged state and lives within a single Ractor, so concurrent proxies in
# separate Ractors must never observe each other's writes. These tests keep that
# safety promise honest.
class ShopifyDraftProxyRactorSafetyTest < Minitest::Test
  def test_a_proxy_can_be_driven_from_a_single_non_main_ractor
    result = ractor_value(Ractor.new { ShopifyDraftProxyRactorSafetyTest.stage_saved_searches(1) })

    assert_equal ["order-0"], result.fetch(:created)
    assert_equal "1:0:2", result.fetch(:version)
  end

  def test_concurrent_ractors_keep_fully_isolated_staged_state
    # Ractor i stages i mutations; its state-version counts only its own staged
    # entries, so version == "i:0:i+1" per Ractor proves there is no bleed either way.
    workers = (1..8).map do |i|
      Ractor.new(i) { |count| ShopifyDraftProxyRactorSafetyTest.stage_saved_searches(count) }
    end
    results = workers.map { |worker| ractor_value(worker) }

    results.each_with_index do |result, index|
      count = index + 1
      assert_equal((0...count).map { |j| "order-#{j}" }, result.fetch(:created))
      assert_equal "#{count}:0:#{count + 1}", result.fetch(:version)
    end
  end

  # Runs entirely inside a worker Ractor and returns only shareable data. A worker
  # Ractor cannot touch the non-shareable Transports::DEFAULT constant, so it supplies
  # its own in-Ractor transport; snapshot mode never invokes it.
  def self.stage_saved_searches(count)
    transport = ->(_request) { { "status" => 200, "headers" => {}, "body" => "{}" } }
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      transport: transport,
    )
    created = (0...count).map do |j|
      response = proxy.process_graphql_request(
        {
          query: <<~GRAPHQL,
            mutation {
              savedSearchCreate(input: { name: "order-#{j}", query: "tag:o#{j}", resourceType: ORDER }) {
                savedSearch { id name }
                userErrors { field message }
              }
            }
          GRAPHQL
        },
      )
      response.body.dig("data", "savedSearchCreate", "savedSearch", "name")
    end
    version = ShopifyDraftProxy::DraftProxy.state_version_of(proxy.dump_state)
    Ractor.make_shareable({ created: created, version: version })
  ensure
    proxy&.dispose
  end

  private

  # Ractor's result accessor was renamed from #take to #value across the Ruby 3.4 ->
  # 4.0 transition; support both so the suite runs on either.
  def ractor_value(ractor)
    ractor.respond_to?(:value) ? ractor.value : ractor.take
  end
end
