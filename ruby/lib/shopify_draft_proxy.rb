# frozen_string_literal: true

require "json"
require "net/http"
require "uri"

require_relative "shopify_draft_proxy/version"
require_relative "shopify_draft_proxy/shopify_draft_proxy_native"

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
  # A failed commit does not persist: the session's staged state is left intact
  # so the commit can be retried after the cause is fixed. This mirrors the JS
  # binding's `DraftProxyCommitError`, keeping the failure contract consistent
  # across language bindings.
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

  # Storage adapters let an embedder persist proxy state somewhere other than
  # process memory — a file, Redis, a database row, S3, ... The proxy stays the
  # single writer of a given adapter; concurrency is expected to come from *many
  # isolated* stores (one key per test/worker), not many writers sharing one.
  #
  # An adapter is any object responding to:
  #   #load        -> a state dump Hash (as returned by `#dump_state`) or nil
  #   #save(dump)  -> persists the given dump Hash
  #
  # Provide one via `ShopifyDraftProxy.create(storage: MyAdapter.new)`. On
  # construction the proxy calls `#load`; a non-nil result rehydrates the proxy
  # (taking precedence over any `state:` seed). By default the proxy then calls
  # `#save` after every request that changed persistable state (see `persist:`).
  module Storage
    # A minimal file-backed adapter that stores the dump as JSON at `path`.
    # Doubles as the reference implementation of the adapter contract.
    class File
      def initialize(path)
        @path = path
      end

      def load
        return nil unless ::File.exist?(@path)

        contents = ::File.read(@path)
        return nil if contents.empty?

        JSON.parse(contents)
      end

      def save(dump)
        ::File.write(@path, JSON.generate(dump))
      end
    end
  end

  class << self
    def create(**options)
      DraftProxy.new(options)
    end
  end

  # A DraftProxy wraps a single mutable Rust store. It is **not** safe to share
  # one instance across threads: each call borrows the store mutably for the
  # whole request — including the outbound transport IO performed during a
  # commit — so a second thread entering the same instance concurrently panics
  # with a Rust `BorrowMutError`. The intended concurrency model is one isolated
  # proxy (and one storage key) per test/worker, never many threads sharing one.
  class DraftProxy
    DEFAULT_API_VERSION = "2025-01"

    # Version token for a brand-new proxy: zero log entries, none settled, and
    # the synthetic id counter at its initial value. Must mirror the Rust
    # `state_version()` format `"<len>:<settled>:<next_synthetic_id>"`.
    PRISTINE_STATE_VERSION = "0:0:1"

    # Inject the default Net::HTTP transport unless the caller supplied one, and
    # pull out the storage-adapter options (which the native runtime does not
    # understand) before constructing, so every construction path ends up with a
    # working transport and an optional persistence wiring.
    class << self
      alias_method :native_new, :new

      def new(options = {})
        options = options.dup
        unless options.key?(:transport) || options.key?("transport")
          options[:transport] = Transports::DEFAULT
        end

        storage = options.delete(:storage) || options.delete("storage")
        persist_mode = options.delete(:persist) || options.delete("persist") || :each_mutation
        unless [:each_mutation, :manual].include?(persist_mode)
          raise ArgumentError, "persist: must be :each_mutation or :manual (got #{persist_mode.inspect})"
        end

        # Storage rehydration wins over an explicit `state:` seed; the seed only
        # applies when the adapter has nothing stored yet.
        if storage
          loaded = storage.load
          options[:state] = loaded unless loaded.nil?
        end

        instance = native_new(options)
        instance.send(:sdp_install_storage, storage, persist_mode, options[:state]) if storage
        instance
      end

      # Compute the state-version token for a state dump Hash: an opaque token
      # that is equal iff two dumps carry the same persistable state — the same
      # value the runtime stamps on every response as the `x-sdp-state-version`
      # header. Callers that drive their own dump/restore persistence (rather than
      # a storage adapter) use it to tell whether an operation changed staged
      # state: compare this token for the seed dump against the response header
      # and persist the new dump only when they differ. Accepts string- or
      # symbol-keyed dumps.
      def state_version_of(dump)
        log = dump["log"] || dump[:log] || {}
        entries = log["entries"] || log[:entries] || []
        settled = entries.count { |entry| (entry["status"] || entry[:status]) != "staged" }
        next_id = dump["nextSyntheticId"] || dump[:nextSyntheticId] || 1
        "#{entries.length}:#{settled}:#{next_id}"
      end
    end

    alias native_process_request process_request
    alias native_process_graphql_request process_graphql_request
    alias native_dump_state dump_state
    alias native_commit commit
    alias native_reset reset
    alias native_restore_state restore_state

    def process_request(request)
      response = self.class.response_from_native(native_process_request(request))
      sdp_maybe_persist(response)
      response
    end

    def process_graphql_request(body, api_version: DEFAULT_API_VERSION, path: nil, headers: {})
      response = self.class.response_from_native(
        native_process_graphql_request(
          body,
          { "api_version" => api_version, "path" => path, "headers" => headers },
        ),
      )
      sdp_maybe_persist(response)
      response
    end

    def dump_state(created_at: nil)
      native_dump_state({ "created_at" => created_at })
    end

    def commit(headers: {})
      result = native_commit({ "headers" => headers })
      # A failed replay leaves the session's staged state untouched so it stays
      # retryable — raise the typed error carrying the full result and do NOT
      # persist (the committed/failed flips the core made in memory are dropped
      # with the disposed proxy).
      raise CommitError, result unless result["ok"]

      # A fully successful commit flips staged log entries to committed in place —
      # a state change that advances the version tuple without adding log entries.
      sdp_persist! if sdp_storage? && @sdp_persist_mode == :each_mutation
      result
    end

    def reset
      result = native_reset
      sdp_persist! if sdp_storage? && @sdp_persist_mode == :each_mutation
      result
    end

    def restore_state(dump)
      result = native_restore_state(dump)
      # A restore replaces staged state wholesale, so the cached version token no
      # longer describes the live store. Re-sync it. Under :each_mutation we also
      # write the restored state through, keeping storage authoritative the same
      # way commit/reset do; under :manual we only refresh the tracker so a later
      # auto/manual persist decision stays correct.
      if sdp_storage?
        if @sdp_persist_mode == :each_mutation
          sdp_persist!
        else
          @sdp_state_version = self.class.state_version_of(native_dump_state({ "created_at" => nil }))
        end
      end
      result
    end

    # Persist the current state to the configured adapter immediately, whatever
    # the persist mode. No-op when no adapter is configured. Use this to control
    # write frequency under `persist: :manual`.
    def persist!
      return nil unless sdp_storage?

      sdp_persist!
      nil
    end

    def dispose
      # No persistence here: in :each_mutation mode every mutating call (request,
      # graphql, commit, reset) has already persisted, so a flush would just
      # rewrite identical state; in :manual mode the caller owns every write.
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

    private

    def sdp_install_storage(storage, persist_mode, effective_state)
      @sdp_storage = storage
      @sdp_persist_mode = persist_mode
      # The store already reflects `effective_state` (loaded or seeded) or a
      # pristine proxy, so record that version without writing it back out.
      @sdp_state_version =
        if effective_state
          self.class.state_version_of(effective_state)
        else
          PRISTINE_STATE_VERSION
        end
    end

    def sdp_storage?
      defined?(@sdp_storage) && !@sdp_storage.nil?
    end

    def sdp_maybe_persist(response)
      return unless sdp_storage?
      return unless @sdp_persist_mode == :each_mutation

      version = sdp_response_version(response)
      return if version.nil? || version == @sdp_state_version

      @sdp_storage.save(native_dump_state({ "created_at" => nil }))
      @sdp_state_version = version
    end

    def sdp_response_version(response)
      headers = response.headers
      return nil if headers.nil?

      headers["x-sdp-state-version"] || headers[:"x-sdp-state-version"]
    end

    def sdp_persist!
      dump = native_dump_state({ "created_at" => nil })
      @sdp_storage.save(dump)
      @sdp_state_version = self.class.state_version_of(dump)
    end
  end
end
