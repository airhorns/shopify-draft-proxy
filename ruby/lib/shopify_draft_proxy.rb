# frozen_string_literal: true

require "json"
require "net/http"
require "uri"

require_relative "shopify_draft_proxy/version"
require_relative "shopify_draft_proxy/shopify_draft_proxy_native"

module ShopifyDraftProxy
  class Error < StandardError; end

  Response = Struct.new(:status, :body, :headers, keyword_init: true)

  # Transports perform the proxy's *outbound* HTTP — the commit replay and any
  # live-hybrid passthrough reads. The native runtime hands the transport a
  # request hash `{ "method", "url", "headers", "body" }` and expects a response
  # hash `{ "status", "headers", "body" }` back. Crucially this work happens in
  # Ruby, so the GVL is released during socket IO and Ruby-level instrumentation
  # (OpenTelemetry, WebMock, VCR, ...) observes the request.
  #
  # A transport is anything responding to `#call`. Provide your own via
  # `ShopifyDraftProxy.create(transport: ->(request) { ... })` — for example to
  # add tracing, retries, or route through a shared connection pool.
  module Transports
    # The default transport: a plain `Net::HTTP` round-trip over the stdlib.
    DEFAULT = lambda do |request|
      uri = URI.parse(request.fetch("url"))
      http = Net::HTTP.new(uri.host, uri.port)
      http.use_ssl = uri.scheme == "https"

      verb = (request["method"] || "POST").to_s.capitalize
      net_request = Net::HTTP.const_get(verb).new(uri)
      (request["headers"] || {}).each { |name, value| net_request[name.to_s] = value.to_s }
      body = request["body"]
      net_request.body = body unless body.nil? || body.empty?

      response = http.request(net_request)
      headers = {}
      response.each_header { |name, value| headers[name] = value }
      { "status" => response.code.to_i, "headers" => headers, "body" => response.body || "" }
    end
  end

  class << self
    def create(**options)
      DraftProxy.new(options)
    end
  end

  class DraftProxy
    DEFAULT_API_VERSION = "2025-01"

    # Inject the default Net::HTTP transport unless the caller supplied one, so
    # every construction path (create, .new) ends up with a working transport
    # while still letting embedders override it.
    class << self
      alias_method :native_new, :new

      def new(options = {})
        options = options.dup
        unless options.key?(:transport) || options.key?("transport")
          options[:transport] = Transports::DEFAULT
        end
        native_new(options)
      end
    end

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
