# frozen_string_literal: true

require "json"
require "tmpdir"
require "minitest/autorun"

require "shopify_draft_proxy"

class ShopifyDraftProxyStorageTest < Minitest::Test
  # Records every save and can hand back a preset dump on load, so tests can
  # assert both the persistence cadence and rehydration.
  class FakeStorage
    attr_reader :saves

    def initialize(initial: nil)
      @initial = initial
      @saves = []
    end

    def load
      @initial
    end

    def save(dump)
      # Deep-copy through JSON so later proxy mutations can't retroactively
      # change an already-captured save.
      @saves << JSON.parse(JSON.generate(dump))
    end

    def last
      @saves.last
    end
  end

  # A storage adapter whose #save can be told to blow up, so tests can exercise
  # the fail-loud persistence contract and its self-healing follow-up write.
  class ExplodingStorage
    attr_reader :saves
    attr_accessor :fail_saves

    def initialize
      @saves = []
      @fail_saves = false
    end

    def load
      nil
    end

    def save(dump)
      raise "storage backend unavailable" if @fail_saves

      @saves << JSON.parse(JSON.generate(dump))
    end

    def last
      @saves.last
    end
  end

  SAVED_SEARCH_CREATE = <<~GRAPHQL
    mutation($name: String!, $query: String!) {
      savedSearchCreate(input: { name: $name, query: $query, resourceType: ORDER }) {
        savedSearch { id name query resourceType }
        userErrors { field message }
      }
    }
  GRAPHQL

  SAVED_SEARCH_UPDATE = <<~GRAPHQL
    mutation($id: ID!, $name: String!) {
      savedSearchUpdate(input: { id: $id, name: $name }) {
        savedSearch { id name }
        userErrors { field message }
      }
    }
  GRAPHQL

  def test_saves_after_a_mutation_but_not_after_a_read
    storage = FakeStorage.new
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      storage: storage,
    )

    # Construction alone must not write.
    assert_equal 0, storage.saves.length

    proxy.process_request(method: "GET", path: "/__meta/health")
    assert_equal 0, storage.saves.length, "a pure read must not persist"

    stage_saved_search(proxy, "First")
    assert_equal 1, storage.saves.length, "a mutation must persist exactly once"

    proxy.get_state
    proxy.process_request(method: "GET", path: "/__meta/state")
    assert_equal 1, storage.saves.length, "reads after a mutation must not persist again"

    stage_saved_search(proxy, "Second")
    assert_equal 2, storage.saves.length

    entries = storage.last.fetch("log").fetch("entries")
    assert_equal 2, entries.length
  ensure
    proxy&.dispose
  end

  def test_loads_state_on_construction
    source = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
    )
    stage_saved_search(source, "Persisted")
    dump = source.dump_state

    storage = FakeStorage.new(initial: dump)
    restored = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      storage: storage,
    )

    assert_equal ["Persisted"], saved_search_names(restored, "Persisted")
    # Rehydration is a load, not a mutation — it must not trigger a save.
    assert_equal 0, storage.saves.length
  ensure
    source&.dispose
    restored&.dispose
  end

  def test_storage_load_takes_precedence_over_state_seed
    from_storage = build_dump_with_saved_search("FromStorage")
    from_seed = build_dump_with_saved_search("FromSeed")

    storage = FakeStorage.new(initial: from_storage)
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      state: from_seed,
      storage: storage,
    )

    assert_equal ["FromStorage"], saved_search_names(proxy, "From")
  ensure
    proxy&.dispose
  end

  def test_manual_mode_only_persists_on_explicit_persist
    storage = FakeStorage.new
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      storage: storage,
      persist: :manual,
    )

    stage_saved_search(proxy, "Manual")
    assert_equal 0, storage.saves.length, "manual mode must not auto-persist"

    proxy.persist!
    assert_equal 1, storage.saves.length
    assert_equal 1, storage.last.fetch("log").fetch("entries").length
  ensure
    proxy&.dispose
  end

  def test_reset_persists_the_cleared_state
    storage = FakeStorage.new
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      storage: storage,
    )

    stage_saved_search(proxy, "Doomed")
    assert_equal 1, storage.last.fetch("log").fetch("entries").length

    proxy.reset
    assert_equal 2, storage.saves.length, "reset must persist"
    assert_equal 0, storage.last.fetch("log").fetch("entries").length
  ensure
    proxy&.dispose
  end

  def test_commit_persists_the_settled_log
    # A stub transport that accepts every replayed mutation, so commit succeeds
    # and flips the staged entry to committed.
    transport = ->(_request) { { "status" => 200, "headers" => {}, "body" => { "data" => {} } } }
    storage = FakeStorage.new
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      storage: storage,
      transport: transport,
    )

    stage_saved_search(proxy, "Committed")
    saves_before_commit = storage.saves.length

    proxy.commit
    assert_operator storage.saves.length, :>, saves_before_commit, "commit must persist"
    statuses = storage.last.fetch("log").fetch("entries").map { |entry| entry.fetch("status") }
    assert_equal ["committed"], statuses
  ensure
    proxy&.dispose
  end

  def test_file_adapter_round_trips_through_disk
    Dir.mktmpdir do |dir|
      path = File.join(dir, "state.json")
      storage = ShopifyDraftProxy::Storage::File.new(path)

      proxy = ShopifyDraftProxy.create(
        read_mode: "snapshot",
        shopify_admin_origin: "https://shopify.example",
        storage: storage,
      )
      stage_saved_search(proxy, "OnDisk")
      proxy.dispose

      assert File.exist?(path), "file adapter should have written the dump"

      reopened = ShopifyDraftProxy.create(
        read_mode: "snapshot",
        shopify_admin_origin: "https://shopify.example",
        storage: ShopifyDraftProxy::Storage::File.new(path),
      )
      assert_equal ["OnDisk"], saved_search_names(reopened, "OnDisk")
      reopened.dispose
    end
  end

  def test_rejects_unknown_persist_mode
    error = assert_raises(ArgumentError) do
      ShopifyDraftProxy.create(
        read_mode: "snapshot",
        shopify_admin_origin: "https://shopify.example",
        storage: FakeStorage.new,
        persist: :sometimes,
      )
    end
    assert_match(/persist:/, error.message)
  end

  def test_restore_state_persists_and_resyncs_tracking_in_each_mutation_mode
    storage = FakeStorage.new
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      storage: storage,
    )

    stage_saved_search(proxy, "Before")
    assert_equal 1, storage.saves.length

    external = build_dump_with_saved_search("Restored")
    proxy.restore_state(external)

    assert_equal 2, storage.saves.length, "restore must persist the restored state"
    assert_equal ["Restored"], saved_search_names(proxy, "Restored")
    assert_equal(
      external.fetch("log").fetch("entries").length,
      storage.last.fetch("log").fetch("entries").length,
      "the persisted dump must reflect the restored state, not the pre-restore state",
    )

    # Tracker was re-synced to the restored state, so a pure read after restore
    # must not trigger another save.
    proxy.process_request(method: "GET", path: "/__meta/state")
    assert_equal 2, storage.saves.length
  ensure
    proxy&.dispose
  end

  def test_restore_state_tracks_without_writing_in_manual_mode
    storage = FakeStorage.new
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      storage: storage,
      persist: :manual,
    )

    proxy.restore_state(build_dump_with_saved_search("Restored"))
    assert_equal 0, storage.saves.length, "manual mode must not auto-persist on restore"

    # The tracker still knows the live state, so an explicit flush writes it.
    proxy.persist!
    assert_equal 1, storage.saves.length
    assert_equal ["Restored"], saved_search_names(proxy, "Restored")
  ensure
    proxy&.dispose
  end

  def test_save_failure_is_fail_loud_and_self_heals
    storage = ExplodingStorage.new
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      storage: storage,
    )

    storage.fail_saves = true
    assert_raises(RuntimeError) do
      proxy.process_graphql_request(
        { query: SAVED_SEARCH_CREATE, variables: { name: "First", query: "tag:first" } },
      )
    end
    assert_equal 0, storage.saves.length, "the failed save must not be recorded"

    # The version token was never advanced (save raised before it was updated),
    # so the cache is now behind the live store. Once the backend recovers, the
    # *next request of any kind* — even a pure read — flushes that stale state,
    # self-healing the skipped write.
    storage.fail_saves = false
    proxy.process_request(method: "GET", path: "/__meta/state")
    assert_equal 1, storage.saves.length, "the next request after recovery flushes the skipped save"
    assert_equal 1, storage.last.fetch("log").fetch("entries").length

    # The mutation had applied in-memory all along, despite its save failing.
    assert_equal ["First"], saved_search_names(proxy, "First")
    assert_equal 1, storage.saves.length, "a read once the tracker is in sync must not save again"
  ensure
    proxy&.dispose
  end

  def test_updating_an_existing_entity_advances_version_and_persists
    storage = FakeStorage.new
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
      storage: storage,
    )

    created = stage_saved_search(proxy, "Original")
    id = created.body.dig("data", "savedSearchCreate", "savedSearch", "id")
    refute_nil id, "savedSearchCreate should return a synthetic id"
    assert_equal 1, storage.saves.length

    updated = proxy.process_graphql_request(
      { query: SAVED_SEARCH_UPDATE, variables: { id: id, name: "Renamed" } },
    )
    assert_equal 200, updated.status
    assert_empty updated.body.dig("data", "savedSearchUpdate", "userErrors")

    # An in-place update of an existing entity is still a persistable state
    # change: the version signal must advance so :each_mutation persists it.
    assert_equal 2, storage.saves.length, "updating an existing entity must persist"
    refute_equal storage.saves[0], storage.saves[1], "the persisted dump must change"
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

  def build_dump_with_saved_search(name)
    proxy = ShopifyDraftProxy.create(
      read_mode: "snapshot",
      shopify_admin_origin: "https://shopify.example",
    )
    stage_saved_search(proxy, name)
    dump = proxy.dump_state
    proxy.dispose
    dump
  end

  def saved_search_names(proxy, query)
    read = proxy.process_graphql_request(
      { query: "{ orderSavedSearches(query: #{JSON.generate(query)}) { nodes { id name } } }" },
    )
    read.body.fetch("data").fetch("orderSavedSearches").fetch("nodes").map { |node| node.fetch("name") }
  end
end
