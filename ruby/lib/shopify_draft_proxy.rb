# frozen_string_literal: true

require "json"

require_relative "shopify_draft_proxy/version"
require_relative "shopify_draft_proxy/shopify_draft_proxy_native"

module ShopifyDraftProxy
  class Error < StandardError; end

  Response = Struct.new(:status, :body, :headers, keyword_init: true)

  class << self
    def create(**options)
      DraftProxy.new(options)
    end
  end

  class DraftProxy
    DEFAULT_API_VERSION = "2025-01"

    alias native_process_request process_request
    alias native_process_graphql_request process_graphql_request
    alias native_dump_state dump_state
    alias native_commit commit

    def process_request(request)
      self.class.response_from_native(native_process_request(request))
    end

    def process_graphql_request(body, api_version: DEFAULT_API_VERSION, path: nil, headers: {})
      self.class.response_from_native(
        native_process_graphql_request(
          body,
          { "api_version" => api_version, "path" => path, "headers" => headers },
        ),
      )
    end

    def dump_state(created_at: nil)
      native_dump_state({ "created_at" => created_at })
    end

    def commit(headers: {})
      native_commit({ "headers" => headers })
    end

    def dispose
      nil
    end

    def origin
      nil
    end

    def self.response_from_native(response)
      Response.new(
        status: response.fetch("status"),
        body: response.fetch("body"),
        headers: response.fetch("headers"),
      )
    end
  end
end
