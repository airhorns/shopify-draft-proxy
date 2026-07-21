# frozen_string_literal: true

require "json"
require "net/http"
require "uri"

require_relative "shopify_draft_proxy/version"

# Install-time-compiled extensions (git/rubygems sources) land in RubyGems' own
# per-gem extension dir on $LOAD_PATH, not in this gem's lib/. Prefer a plain
# load-path require so consumers find it, and fall back to require_relative for
# local `rake native:build` copies that sit next to this file.
begin
  require "shopify_draft_proxy/shopify_draft_proxy_native"
rescue LoadError
  require_relative "shopify_draft_proxy/shopify_draft_proxy_native"
end

module ShopifyDraftProxy
  class Error < StandardError; end

  # Raised by {#commit} when the session's staged mutations do not all replay
  # successfully — the upstream returned a non-2xx status or GraphQL errors for
  # at least one staged mutation, so the replay stopped. It carries the full
  # commit result so callers can inspect exactly what happened without parsing a
  # message string:
  #
  #   error.result["committed"] # => count replayed before the stop
  #   error.result["failed"]    # => count that failed (>= 1)
  #   error.result["stopIndex"] # => log index the replay stopped at
  #   error.result["attempts"]  # => per-mutation records; the failing one carries
  #                             #    { "response" => { "status", "body" } } — the
  #                             #    real upstream response that caused the stop
  #   error.result["error"]     # => human-readable reason for the stop
  #
  # A failed commit leaves the session's staged state intact so the commit can be
  # retried after the cause is fixed. This mirrors the JS binding's
  # `DraftProxyCommitError`, keeping the failure contract consistent across
  # language bindings.
  class CommitError < Error
    # @return [Hash] the structured commit result (`"ok" => false`, plus
    #   `committed`/`failed`/`stopIndex`/`attempts`/`error`).
    attr_reader :result

    def initialize(result)
      @result = result
      super(result["error"] || "commit stopped before all staged mutations were replayed")
    end
  end

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

    # Version token for a brand-new proxy: zero log entries, none settled, and
    # the synthetic id counter at its initial value. Mirrors the Rust
    # `state_version()` format `"<len>:<change_counter>:<next_synthetic_id>"`, so it is
    # the baseline to compare a userland dump/restore seed of `nil` against.
    PRISTINE_STATE_VERSION = "0:0:1"

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

      # Compute the state-version token for a state dump Hash: an opaque token
      # that is equal iff two dumps carry the same persistable state — the same
      # value the runtime stamps on every response as the `x-sdp-state-version`
      # header. Callers that drive their own dump/restore persistence use it to
      # tell whether an operation changed staged state: compare this token for
      # the seed dump against the response header and persist the new dump only
      # when they differ. Accepts string- or symbol-keyed dumps.
      def state_version_of(dump)
        log = dump["log"] || dump[:log] || {}
        entries = log["entries"] || log[:entries] || []
        settled = entries.count { |entry| (entry["status"] || entry[:status]) != "staged" }
        revision = dump["stateRevision"] || dump[:stateRevision] || 0
        next_id = dump["nextSyntheticId"] || dump[:nextSyntheticId] || 1
        "#{entries.length}:#{settled + revision}:#{next_id}"
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
      result = native_commit({ "headers" => headers })
      # A failed replay leaves the session's staged state untouched so it stays
      # retryable — raise the typed error carrying the full result. The
      # committed/failed flips the core made in memory are dropped with the
      # disposed proxy.
      raise CommitError, result unless result["ok"]

      result
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
