# frozen_string_literal: true

require_relative "test_helper"

# End-to-end tests for the catalog importer, driven through the real
# `shopify_api` client against the in-process draft proxy.
class CatalogImporterTest < Minitest::Test
  SAMPLE_CATALOG = JSON.parse(File.read(File.expand_path("../data/catalog.json", __dir__)))

  def setup
    ProxyHarness.setup_context!
    @proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      unsupported_mutation_mode: "reject",
    )
    ProxyHarness.bridge!(@proxy)
    @importer = CatalogImporter.new(client: ProxyHarness.client)
  end

  def teardown
    @proxy&.dispose
  end

  def test_imports_catalog_and_reads_each_product_back
    imported = @importer.import(SAMPLE_CATALOG)

    assert_equal ["Aurora Tee", "Borealis Mug", "Tundra Beanie"], imported.map(&:title)
    imported.each do |product|
      assert_includes product.id, "shopify-draft-proxy=synthetic"
    end

    # Read-after-write: every staged product is readable with faithful fields.
    aurora = @importer.fetch(imported.first.id)
    assert_equal "Aurora Tee", aurora.fetch("title")
    assert_equal "aurora-tee", aurora.fetch("handle")
    assert_equal "Northwind", aurora.fetch("vendor")
    assert_equal "Shirts", aurora.fetch("productType")
    assert_equal "ACTIVE", aurora.fetch("status")
    assert_includes aurora.fetch("descriptionHtml"), "northern-lights"
  end

  def test_extra_tags_are_applied_and_visible_on_read
    imported = @importer.import(SAMPLE_CATALOG)
    aurora = @importer.fetch(imported.first.id)

    # "featured" was applied via a follow-up tagsAdd; "summer"/"organic" at create.
    assert_equal ["featured", "organic", "summer"], aurora.fetch("tags").sort
  end

  def test_products_query_reflects_staged_writes
    @importer.import(SAMPLE_CATALOG)

    # The staged products are visible through Shopify-like product search
    # filters, so an importer can rely on read-after-write filtered checks.
    titles = @importer.search("vendor:Northwind").map { |p| p.fetch("title") }.sort
    assert_equal ["Aurora Tee", "Borealis Mug"], titles

    glacier = @importer.search("vendor:Glacier Goods").map { |p| p.fetch("title") }.sort
    assert_equal ["Tundra Beanie"], glacier
  end

  def test_surfaces_user_errors_as_import_error
    # An empty title is rejected by the API with a domain `userError`, which the
    # importer surfaces as an ImportError carrying the structured errors.
    error = assert_raises(CatalogImporter::ImportError) do
      @importer.import([{ "title" => "", "vendor" => "Northwind" }])
    end

    assert_match(/Title can't be blank/, error.message)
    refute_empty error.user_errors
    assert_equal ["title"], error.user_errors.first.fetch("field")
  end

  def test_surfaces_graphql_validation_errors_as_import_error
    error = assert_raises(CatalogImporter::ImportError) do
      @importer.import([{ "title" => "Bad Status", "status" => "NOPE" }])
    end

    assert_match(/GraphQL errors/, error.message)
    assert_match(/ACTIVE/, error.message)
  end

  def test_saves_a_product_saved_search
    saved = @importer.save_search(name: "Northwind active", query: "vendor:Northwind status:active")

    assert_equal "Northwind active", saved.fetch("name")
    assert_equal "PRODUCT", saved.fetch("resourceType")
    assert_includes saved.fetch("id"), "shopify-draft-proxy=synthetic"
  end

  def test_each_proxy_instance_is_isolated
    @importer.import([{ "title" => "Only Here", "vendor" => "Solo" }])
    assert_equal ["Only Here"], @importer.search("vendor:Solo").map { |p| p.fetch("title") }

    # A second, independent instance starts empty — staged writes do not leak
    # across `ShopifyDraftProxy.create` boundaries.
    other_proxy = ShopifyDraftProxy.create(read_mode: "snapshot", shopify_admin_origin: "https://shopify.example")
    ProxyHarness.bridge!(other_proxy)
    other_importer = CatalogImporter.new(client: ProxyHarness.client)
    assert_empty other_importer.search("vendor:Solo")

    # Switching back proves the first instance kept its state all along.
    ProxyHarness.bridge!(@proxy)
    assert_equal ["Only Here"], @importer.search("vendor:Solo").map { |p| p.fetch("title") }
  ensure
    other_proxy&.dispose
  end

  def test_dump_state_serializes_the_draft_buffer_and_restores_across_a_string_round_trip
    @importer.import(SAMPLE_CATALOG)

    dump = @proxy.dump_state(created_at: "2026-06-13T00:00:00.000Z")
    assert_equal ShopifyDraftProxy::DRAFT_PROXY_STATE_DUMP_SCHEMA, dump.fetch("schema")
    assert_equal "2026-06-13T00:00:00.000Z", dump.fetch("createdAt")

    # Simulate persistence: serialize the draft buffer to a JSON string, drop
    # the live instance, then rehydrate from the parsed string — exactly what a
    # job that checkpoints to Redis/disk/a queue would do between runs.
    serialized = JSON.generate(dump)
    assert_kind_of String, serialized
    rehydrated = JSON.parse(serialized)
    assert_equal dump, rehydrated, "round-tripping through JSON must be lossless"

    # The serialized buffer carries the staged writes themselves, not just
    # config: the three products and the four-entry mutation log are all present.
    staged_products = rehydrated.dig("state", "stagedState", "products")
    assert_equal 3, staged_products.length
    assert_equal(
      ["Aurora Tee", "Borealis Mug", "Tundra Beanie"],
      staged_products.values.map { |product| product.fetch("title") }.sort,
    )
    log_entries = rehydrated.dig("log", "entries")
    assert_equal 4, log_entries.length
    assert(log_entries.all? { |entry| entry.fetch("status") == "staged" })

    # A brand-new instance rehydrated from the parsed string reads every product
    # back faithfully — the importer cannot tell it from the original.
    restored_proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: ProxyHarness::UPSTREAM_ORIGIN,
      state: rehydrated,
    )
    ProxyHarness.bridge!(restored_proxy)
    restored_importer = CatalogImporter.new(client: ProxyHarness.client)

    titles = restored_importer.search("vendor:Northwind").map { |p| p.fetch("title") }.sort
    assert_equal ["Aurora Tee", "Borealis Mug"], titles
    glacier = restored_importer.search("vendor:Glacier Goods").map { |p| p.fetch("title") }.sort
    assert_equal ["Tundra Beanie"], glacier

    aurora_id = restored_importer.search("vendor:Northwind")
      .find { |p| p.fetch("title") == "Aurora Tee" }.fetch("id")
    aurora = restored_importer.fetch(aurora_id)
    assert_equal "aurora-tee", aurora.fetch("handle")
    assert_equal ["featured", "organic", "summer"], aurora.fetch("tags").sort

    # The restored buffer is still committable: its four staged mutations replay
    # upstream and the log flips to committed.
    replayed = ProxyHarness.capture_upstream_replays!
    result = restored_proxy.commit(headers: { "X-Shopify-Access-Token" => ProxyHarness::ACCESS_TOKEN })
    assert_equal true, result.fetch("ok")
    assert_equal 4, result.fetch("committed")
    assert_equal 4, replayed.length
    assert(restored_proxy.get_log.fetch("entries").all? { |entry| entry.fetch("status") == "committed" })
  ensure
    restored_proxy&.dispose
  end

  def test_staged_mutations_are_recorded_in_the_log
    @importer.import([{ "title" => "Aurora Tee", "vendor" => "Northwind", "extra_tags" => ["featured"] }])

    entries = @proxy.get_log.fetch("entries")
    assert_equal 2, entries.length
    assert(entries.all? { |entry| entry["status"] == "staged" })

    # The twin understands each operation: the parsed root field shows up under
    # `interpreted`, which is the field to read for "what did this stage do".
    root_fields = entries.map { |entry| entry.dig("interpreted", "primaryRootField") }
    assert_equal ["productCreate", "tagsAdd"], root_fields

    # The *top-level* `operationName` column is nil, though: it mirrors the
    # explicit `operationName` field of the request body, which the official
    # client does not send. Consumers should rely on `interpreted` instead.
    assert(entries.all? { |entry| entry["operationName"].nil? })
  end

  def test_commit_replays_staged_mutations_upstream
    # The commit replay now runs its HTTP in Ruby (Net::HTTP) via the default
    # transport, so WebMock captures it in-process — no separate OS process.
    replayed = ProxyHarness.capture_upstream_replays!
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: ProxyHarness::UPSTREAM_ORIGIN,
    )
    ProxyHarness.bridge!(proxy)
    importer = CatalogImporter.new(client: ProxyHarness.client)

    # 3 products + 1 follow-up tagsAdd = 4 staged mutations.
    importer.import(SAMPLE_CATALOG)
    assert_equal 4, proxy.get_log.fetch("entries").length

    result = proxy.commit(headers: { "X-Shopify-Access-Token" => ProxyHarness::ACCESS_TOKEN })
    assert_equal true, result.fetch("ok")
    assert_equal 4, result.fetch("committed")

    # The proxy replayed each staged mutation to the upstream as a real POST.
    assert_equal 4, replayed.length
    assert(replayed.all? { |req| req["path"] == "/admin/api/#{ProxyHarness::API_VERSION}/graphql.json" })
    assert(replayed.all? { |req| req["token"] == ProxyHarness::ACCESS_TOKEN })
    replayed_queries = replayed.map { |req| req.dig("body", "query") }
    assert(replayed_queries.any? { |q| q&.include?("productCreate") })
    assert(replayed_queries.any? { |q| q&.include?("tagsAdd") })

    assert(proxy.get_log.fetch("entries").all? { |entry| entry["status"] == "committed" })
  ensure
    proxy&.dispose
  end

  def test_commit_runs_through_a_custom_ruby_transport
    # Embedders can supply their own transport — e.g. to add tracing, retries,
    # or route through a pooled connection. Here the transport records a span
    # per replay and synthesizes the upstream response itself, proving the
    # outbound HTTP is fully owned by host-language Ruby.
    spans = []
    transport = lambda do |request|
      spans << { method: request.fetch("method"), url: request.fetch("url") }
      {
        "status" => 200,
        "headers" => { "content-type" => "application/json" },
        "body" => JSON.generate("data" => {}),
      }
    end

    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: ProxyHarness::UPSTREAM_ORIGIN,
      transport: transport,
    )
    ProxyHarness.bridge!(proxy)
    importer = CatalogImporter.new(client: ProxyHarness.client)

    importer.import(SAMPLE_CATALOG)
    result = proxy.commit(headers: { "X-Shopify-Access-Token" => ProxyHarness::ACCESS_TOKEN })

    assert_equal true, result.fetch("ok")
    assert_equal 4, result.fetch("committed")
    assert_equal 4, spans.length
    assert(spans.all? { |span| span[:method] == "POST" })
    assert(spans.all? { |span| span[:url].end_with?("/graphql.json") })
  ensure
    proxy&.dispose
  end
end
