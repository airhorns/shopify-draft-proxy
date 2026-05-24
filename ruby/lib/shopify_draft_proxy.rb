# frozen_string_literal: true

require "json"
require "net/http"
require "socket"
require "timeout"
require "uri"

require_relative "shopify_draft_proxy/version"

module ShopifyDraftProxy
  class Error < StandardError; end

  Response = Struct.new(:status, :body, :headers, keyword_init: true)

  @active_children = []

  class << self
    attr_reader :active_children

    def create(**options)
      DraftProxy.new(**options)
    end
  end

  at_exit do
    ShopifyDraftProxy.active_children.each do |child|
      Process.kill("TERM", -child[:pid])
    rescue Errno::ESRCH
      nil
    end
  end

  class DraftProxy
    DEFAULT_API_VERSION = "2025-01"
    DEFAULT_STARTUP_TIMEOUT = 15

    attr_reader :origin

    def initialize(
      read_mode:,
      shopify_admin_origin:,
      port: 0,
      snapshot_path: nil,
      unsupported_mutation_mode: "passthrough",
      bulk_operation_run_mutation_max_input_file_size_bytes: 104_857_600,
      state: nil,
      repo_root: nil,
      server_bin: ENV["SHOPIFY_DRAFT_PROXY_SERVER_BIN"],
      cargo_bin: ENV.fetch("CARGO", "cargo"),
      startup_timeout: DEFAULT_STARTUP_TIMEOUT
    )
      @port = port.zero? ? self.class.allocate_port : port
      @origin = "http://127.0.0.1:#{@port}"
      @output = +""
      @output_lock = Mutex.new
      @repo_root = repo_root || self.class.default_repo_root
      @server_bin = server_bin
      @cargo_bin = cargo_bin
      @pid = spawn_runtime(
        read_mode: read_mode,
        shopify_admin_origin: shopify_admin_origin,
        snapshot_path: snapshot_path,
        unsupported_mutation_mode: unsupported_mutation_mode,
        bulk_operation_run_mutation_max_input_file_size_bytes: bulk_operation_run_mutation_max_input_file_size_bytes,
      )
      ShopifyDraftProxy.active_children << { pid: @pid }
      wait_for_runtime(startup_timeout)
      restore_state(state) unless state.nil?
    end

    def dispose
      return if @pid.nil?

      ShopifyDraftProxy.active_children.reject! { |child| child[:pid] == @pid }
      begin
        Process.kill("TERM", -@pid)
        Timeout.timeout(5) { Process.wait(@pid) }
      rescue Errno::ECHILD, Errno::ESRCH
        nil
      rescue Timeout::Error
        Process.kill("KILL", -@pid)
        Process.wait(@pid)
      ensure
        @pid = nil
      end
    end

    def process_request(request)
      self.class.http_request(@origin, request)
    end

    def process_graphql_request(body, api_version: DEFAULT_API_VERSION, path: nil, headers: {})
      process_request(
        method: "POST",
        path: path || "/admin/api/#{api_version}/graphql.json",
        headers: { "content-type" => "application/json" }.merge(headers),
        body: body,
      )
    end

    def reset
      process_request(method: "POST", path: "/__meta/reset")
    end

    def get_config
      process_request(method: "GET", path: "/__meta/config").body
    end

    def get_log
      process_request(method: "GET", path: "/__meta/log").body
    end

    def get_state
      process_request(method: "GET", path: "/__meta/state").body
    end

    def dump_state(created_at: nil)
      process_request(
        method: "POST",
        path: "/__meta/dump",
        headers: { "content-type" => "application/json" },
        body: created_at.nil? ? nil : { createdAt: created_at },
      ).body
    end

    def restore_state(dump)
      response = process_request(
        method: "POST",
        path: "/__meta/restore",
        headers: { "content-type" => "application/json" },
        body: dump,
      )
      raise Error, "DraftProxy.restore_state failed with status #{response.status}" unless response.status == 200

      response
    end

    def commit(headers: {})
      response = process_request(method: "POST", path: "/__meta/commit", headers: headers)
      body = response.body
      raise Error, "DraftProxy.commit failed: #{body.inspect}" unless body.is_a?(Hash) && body["ok"]

      body
    end

    def self.allocate_port
      server = TCPServer.new("127.0.0.1", 0)
      server.addr[1]
    ensure
      server&.close
    end

    def self.default_repo_root
      File.expand_path("../..", __dir__)
    end

    def self.http_request(origin, request)
      uri = URI.join(origin, request.fetch(:path))
      http = Net::HTTP.new(uri.host, uri.port)
      klass = Net::HTTP.const_get(request.fetch(:method).to_s.capitalize)
      raw = klass.new(uri)
      (request[:headers] || {}).each { |key, value| raw[key] = Array(value).join(",") unless value.nil? }
      body = request[:body]
      raw.body = body.is_a?(String) ? body : JSON.generate(body) unless body.nil?
      response = http.request(raw)
      parsed_body = parse_body(response.body)
      Response.new(status: response.code.to_i, body: parsed_body, headers: response.each_header.to_h)
    end

    def self.parse_body(body)
      return nil if body.nil? || body.empty?

      JSON.parse(body)
    rescue JSON::ParserError
      body
    end

    private

    def spawn_runtime(
      read_mode:,
      shopify_admin_origin:,
      snapshot_path:,
      unsupported_mutation_mode:,
      bulk_operation_run_mutation_max_input_file_size_bytes:
    )
      reader, writer = IO.pipe
      command = if @server_bin && !@server_bin.empty?
        [@server_bin]
      else
        [@cargo_bin, "run", "--bin", "shopify-draft-proxy-server", "--quiet"]
      end
      env = {
        "PORT" => @port.to_s,
        "SHOPIFY_ADMIN_ORIGIN" => shopify_admin_origin,
        "READ_MODE" => read_mode,
        "UNSUPPORTED_MUTATION_MODE" => unsupported_mutation_mode,
        "BULK_OPERATION_RUN_MUTATION_MAX_INPUT_FILE_SIZE_BYTES" => bulk_operation_run_mutation_max_input_file_size_bytes.to_s,
      }
      env["SNAPSHOT_PATH"] = snapshot_path unless snapshot_path.nil?
      pid = Process.spawn(env, *command, chdir: @repo_root, out: writer, err: writer, pgroup: true)
      writer.close
      Thread.new do
        reader.each_line { |line| append_output(line) }
      ensure
        reader.close
      end
      pid
    end

    def append_output(text)
      @output_lock.synchronize { @output << text }
    end

    def output
      @output_lock.synchronize { @output.dup }
    end

    def wait_for_runtime(timeout_seconds)
      deadline = Process.clock_gettime(Process::CLOCK_MONOTONIC) + timeout_seconds
      loop do
        return if output.include?("shopify-draft-proxy rust runtime listening")
        return if health_ready?

        _, status = Process.waitpid2(@pid, Process::WNOHANG)
        raise Error, "Rust DraftProxy runtime exited before listening:\n#{output}" unless status.nil?
        raise Error, "Rust DraftProxy runtime did not start before timeout:\n#{output}" if Process.clock_gettime(Process::CLOCK_MONOTONIC) >= deadline

        sleep 0.1
      end
    end

    def health_ready?
      response = process_request(method: "GET", path: "/__meta/health")
      response.status == 200
    rescue StandardError
      false
    end
  end
end
