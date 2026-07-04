# frozen_string_literal: true

require "json"
require "minitest/autorun"

require "shopify_draft_proxy"

# The state-version token lets a caller driving its own dump/restore persistence
# decide whether an operation changed staged state without diffing whole dumps:
# `DraftProxy.state_version_of(dump)` must equal the `x-sdp-state-version` header
# the runtime stamps on the response for that same state.
class ShopifyDraftProxyStateVersionTest < Minitest::Test
  SAVED_SEARCH_CREATE = <<~GRAPHQL
    mutation($name: String!, $query: String!) {
      savedSearchCreate(input: { name: $name, query: $query, resourceType: ORDER }) {
        savedSearch { id name query resourceType }
        userErrors { field message }
      }
    }
  GRAPHQL

  def test_state_version_of_matches_the_runtime_response_header
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
    )

    # A pristine dump's token is the pristine constant and equals the header the
    # runtime stamps on a pure read.
    health = proxy.process_request(method: "GET", path: "/__meta/health")
    assert_equal(
      ShopifyDraftProxy::DraftProxy::PRISTINE_STATE_VERSION,
      ShopifyDraftProxy::DraftProxy.state_version_of(proxy.dump_state),
    )
    assert_equal(
      health.headers.fetch("x-sdp-state-version"),
      ShopifyDraftProxy::DraftProxy.state_version_of(proxy.dump_state),
    )

    # After a staged mutation the token advances and still equals the header the
    # runtime stamped for that same state, so a userland dump/restore caller can
    # detect the change from the dump alone.
    staged = stage_saved_search(proxy, "Versioned")
    assert_equal(
      staged.headers.fetch("x-sdp-state-version"),
      ShopifyDraftProxy::DraftProxy.state_version_of(proxy.dump_state),
    )
  ensure
    proxy&.dispose
  end

  private

  def stage_saved_search(proxy, name)
    response = proxy.process_graphql_request(
      { query: SAVED_SEARCH_CREATE, variables: { name: name, query: "tag:#{name.downcase}" } },
    )
    assert_equal 200, response.status
    response
  end
end
