//// Read-path tests for `proxy/localization`. Exercises the always-on
//// surfaces: availableLocales (default catalog), shopLocales (default
//// + custom + filter), translatableResource(s) (null/empty when no
//// Products + translations have been staged), and the predicate helpers.

import gleam/dict
import gleam/json
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/localization
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type CollectionRecord, type ProductSeoRecord, CollectionRecord, LocaleRecord,
  ProductRecord, ProductSeoRecord, ShopLocaleRecord, TranslationRecord,
}

fn run(store_in: store.Store, query: String) -> String {
  let assert Ok(data) =
    localization.handle_localization_query(store_in, query, dict.new())
  json.to_string(data)
}

// ---------- predicates ----------

pub fn is_localization_query_root_test() {
  assert localization.is_localization_query_root("availableLocales")
  assert localization.is_localization_query_root("shopLocales")
  assert localization.is_localization_query_root("translatableResource")
  assert localization.is_localization_query_root("translatableResources")
  assert localization.is_localization_query_root("translatableResourcesByIds")
  assert !localization.is_localization_query_root("shopLocaleEnable")
  assert !localization.is_localization_query_root("locale")
}

pub fn is_localization_mutation_root_test() {
  assert localization.is_localization_mutation_root("shopLocaleEnable")
  assert localization.is_localization_mutation_root("shopLocaleUpdate")
  assert localization.is_localization_mutation_root("shopLocaleDisable")
  assert localization.is_localization_mutation_root("translationsRegister")
  assert localization.is_localization_mutation_root("translationsRemove")
  assert !localization.is_localization_mutation_root("availableLocales")
}

// ---------- availableLocales ----------

pub fn available_locales_default_catalog_test() {
  let result = run(store.new(), "{ availableLocales { isoCode name } }")
  // Default catalog mirrors Shopify's broad alternate-locale catalog.
  assert string.contains(result, "{\"isoCode\":\"af\",\"name\":\"Afrikaans\"}")
  assert string.contains(result, "{\"isoCode\":\"fr\",\"name\":\"French\"}")
  assert string.contains(result, "{\"isoCode\":\"tr\",\"name\":\"Turkish\"}")
  assert string.contains(result, "{\"isoCode\":\"zu\",\"name\":\"Zulu\"}")
}

pub fn available_locales_overridden_by_store_test() {
  let s =
    store.replace_base_available_locales(store.new(), [
      LocaleRecord(iso_code: "en", name: "English"),
      LocaleRecord(iso_code: "ja", name: "Japanese"),
    ])
  let result = run(s, "{ availableLocales { isoCode } }")
  assert result
    == "{\"availableLocales\":[{\"isoCode\":\"en\"},{\"isoCode\":\"ja\"}]}"
}

// ---------- shopLocales ----------

pub fn shop_locales_default_test() {
  let result =
    run(store.new(), "{ shopLocales { locale name primary published } }")
  assert result
    == "{\"shopLocales\":[{\"locale\":\"en\",\"name\":\"English\",\"primary\":true,\"published\":true}]}"
}

pub fn shop_locales_with_staged_record_test() {
  let s = store.new()
  let #(_, s) =
    store.stage_shop_locale(
      s,
      ShopLocaleRecord(
        locale: "fr",
        name: "French",
        primary: False,
        published: False,
        market_web_presence_ids: [],
      ),
    )
  let result = run(s, "{ shopLocales { locale name primary published } }")
  // The staged "fr" record shadows the default "en" — the store
  // returns it directly without merging the default.
  assert result
    == "{\"shopLocales\":[{\"locale\":\"fr\",\"name\":\"French\",\"primary\":false,\"published\":false}]}"
}

pub fn shop_locales_published_filter_test() {
  let result = run(store.new(), "{ shopLocales(published: false) { locale } }")
  // Default catalog has only one ShopLocale (en, published=true).
  assert result == "{\"shopLocales\":[]}"
}

// ---------- translatableResource ----------

pub fn translatable_resource_unknown_returns_null_for_synthesized_test() {
  // Without translations staged for the resourceId AND without a
  // Products domain, no resource can be reconstructed → null.
  let result =
    run(
      store.new(),
      "{ translatableResource(resourceId: \"gid://shopify/Product/1\") { resourceId } }",
    )
  assert result == "{\"translatableResource\":null}"
}

pub fn translatable_resource_synthesized_from_staged_translation_test() {
  // Staging a translation populates the store such that the resourceId
  // becomes synthetically discoverable via translatableResource.
  let s = store.new()
  let #(_, s) =
    store.stage_translation(
      s,
      types.TranslationRecord(
        resource_id: "gid://shopify/Product/1",
        key: "title",
        locale: "fr",
        value: "Bonjour",
        translatable_content_digest: "abc",
        market_id: None,
        updated_at: "2024-01-01T00:00:00.000Z",
        outdated: False,
      ),
    )
  let result =
    run(
      s,
      "{ translatableResource(resourceId: \"gid://shopify/Product/1\") { resourceId translations(locale: \"fr\") { key value locale } } }",
    )
  assert result
    == "{\"translatableResource\":{\"resourceId\":\"gid://shopify/Product/1\",\"translations\":[{\"key\":\"title\",\"value\":\"Bonjour\",\"locale\":\"fr\"}]}}"
}

pub fn translatable_resources_connection_empty_test() {
  // No Products → empty connection regardless of resourceType.
  let result =
    run(
      store.new(),
      "{ translatableResources(first: 10, resourceType: PRODUCT) { nodes { resourceId } } }",
    )
  assert result == "{\"translatableResources\":{\"nodes\":[]}}"
}

pub fn translatable_resources_include_effective_products_test() {
  let product =
    ProductRecord(
      id: "gid://shopify/Product/1",
      legacy_resource_id: None,
      title: "The Inventory Not Tracked Snowboard",
      handle: "the-inventory-not-tracked-snowboard",
      status: "ACTIVE",
      vendor: None,
      product_type: Some("snowboard"),
      tags: [],
      price_range_min: None,
      price_range_max: None,
      total_variants: None,
      has_only_default_variant: None,
      has_out_of_stock_variants: None,
      total_inventory: None,
      tracks_inventory: None,
      created_at: None,
      updated_at: None,
      published_at: None,
      description_html: "",
      online_store_preview_url: None,
      template_suffix: None,
      seo: ProductSeoRecord(title: None, description: None),
      category: None,
      publication_ids: [],
      contextual_pricing: None,
      cursor: None,
      combined_listing_role: None,
      combined_listing_parent_id: None,
      combined_listing_child_ids: [],
    )
  let s = store.upsert_base_products(store.new(), [product])
  let result =
    run(
      s,
      "{ translatableResources(first: 10, resourceType: PRODUCT) { nodes { resourceId translatableContent { key value locale type } } } }",
    )
  assert result
    == "{\"translatableResources\":{\"nodes\":[{\"resourceId\":\"gid://shopify/Product/1\",\"translatableContent\":[{\"key\":\"title\",\"value\":\"The Inventory Not Tracked Snowboard\",\"locale\":\"en\",\"type\":\"SINGLE_LINE_TEXT_FIELD\"},{\"key\":\"handle\",\"value\":\"the-inventory-not-tracked-snowboard\",\"locale\":\"en\",\"type\":\"URI\"},{\"key\":\"product_type\",\"value\":\"snowboard\",\"locale\":\"en\",\"type\":\"SINGLE_LINE_TEXT_FIELD\"}]}]}}"
}

pub fn translatable_resources_include_effective_collections_test() {
  let s =
    store.new()
    |> store.upsert_base_collections([
      collection_record(
        "gid://shopify/Collection/1",
        "Summer Hats",
        "summer-hats",
        Some("<p>Shade-ready hats.</p>"),
        ProductSeoRecord(
          title: Some("Sun hats"),
          description: Some("Wide brim and cap styles."),
        ),
      ),
    ])
  let result =
    run(
      s,
      "{ translatableResources(first: 10, resourceType: COLLECTION) { nodes { resourceId translatableContent { key value digest locale type } } } }",
    )
  assert result
    == "{\"translatableResources\":{\"nodes\":[{\"resourceId\":\"gid://shopify/Collection/1\",\"translatableContent\":[{\"key\":\"title\",\"value\":\"Summer Hats\",\"digest\":\"76ea81053fa568ea139bf950083fb6a073e6a2e123ae897fba5109c2cc6a8883\",\"locale\":\"en\",\"type\":\"SINGLE_LINE_TEXT_FIELD\"},{\"key\":\"handle\",\"value\":\"summer-hats\",\"digest\":\"a4c27b620d608aa7bd8bd8c3c67cdde9b5217b3aabf447813eccd452c3487d0f\",\"locale\":\"en\",\"type\":\"URI\"},{\"key\":\"body_html\",\"value\":\"<p>Shade-ready hats.</p>\",\"digest\":\"71e67d60d121e7ecb39614f1bc2e67b21b8634b32d1805592f33cfd49a0dc437\",\"locale\":\"en\",\"type\":\"HTML\"},{\"key\":\"meta_title\",\"value\":\"Sun hats\",\"digest\":\"caa78e4b8ae1ab01d7d7c970d48558ca1eb17e30bafe9fae57c1e6bd1fe7a96a\",\"locale\":\"en\",\"type\":\"SINGLE_LINE_TEXT_FIELD\"},{\"key\":\"meta_description\",\"value\":\"Wide brim and cap styles.\",\"digest\":\"2e17d64ac9234b787db9a59d18c6ada32d79466c5543b01a1450b250d1aece33\",\"locale\":\"en\",\"type\":\"MULTI_LINE_TEXT_FIELD\"}]}]}}"
}

pub fn translatable_resources_include_seeded_source_markers_test() {
  let s = store.new()
  let #(_, s) =
    store.stage_translation(
      s,
      TranslationRecord(
        resource_id: "gid://shopify/Product/2",
        key: "title",
        locale: "__source",
        value: "Source title",
        translatable_content_digest: "digest-title",
        market_id: None,
        updated_at: "1970-01-01T00:00:00Z",
        outdated: False,
      ),
    )
  let result =
    run(
      s,
      "{ translatableResources(first: 10, resourceType: PRODUCT) { nodes { resourceId translatableContent { key value digest locale type } } } }",
    )
  assert result
    == "{\"translatableResources\":{\"nodes\":[{\"resourceId\":\"gid://shopify/Product/2\",\"translatableContent\":[{\"key\":\"title\",\"value\":\"Source title\",\"digest\":\"digest-title\",\"locale\":\"en\",\"type\":\"SINGLE_LINE_TEXT_FIELD\"}]}]}}"
}

pub fn translatable_resources_by_ids_finds_synthesized_test() {
  // Staging a translation makes that id reachable through
  // translatableResourcesByIds even when the underlying Product
  // record isn't in the store.
  let s = store.new()
  let #(_, s) =
    store.stage_translation(
      s,
      types.TranslationRecord(
        resource_id: "gid://shopify/Product/42",
        key: "title",
        locale: "fr",
        value: "Salut",
        translatable_content_digest: "x",
        market_id: None,
        updated_at: "2024-01-01T00:00:00.000Z",
        outdated: False,
      ),
    )
  let result =
    run(
      s,
      "{ translatableResourcesByIds(first: 10, resourceIds: [\"gid://shopify/Product/42\", \"gid://shopify/Product/missing\"]) { nodes { resourceId } } }",
    )
  assert result
    == "{\"translatableResourcesByIds\":{\"nodes\":[{\"resourceId\":\"gid://shopify/Product/42\"}]}}"
}

pub fn translatable_resources_by_ids_finds_collection_source_marker_test() {
  let s = store.new()
  let #(_, s) =
    store.stage_translation(
      s,
      TranslationRecord(
        resource_id: "gid://shopify/Collection/42",
        key: "title",
        locale: "__source",
        value: "Collection source",
        translatable_content_digest: "digest-title",
        market_id: None,
        updated_at: "1970-01-01T00:00:00Z",
        outdated: False,
      ),
    )
  let result =
    run(
      s,
      "{ translatableResourcesByIds(first: 10, resourceIds: [\"gid://shopify/Collection/42\"]) { nodes { resourceId translatableContent { key value digest type } } } }",
    )
  assert result
    == "{\"translatableResourcesByIds\":{\"nodes\":[{\"resourceId\":\"gid://shopify/Collection/42\",\"translatableContent\":[{\"key\":\"title\",\"value\":\"Collection source\",\"digest\":\"digest-title\",\"type\":\"SINGLE_LINE_TEXT_FIELD\"}]}]}}"
}

fn collection_record(
  id: String,
  title: String,
  handle: String,
  description_html: Option(String),
  seo: ProductSeoRecord,
) -> CollectionRecord {
  CollectionRecord(
    id: id,
    legacy_resource_id: None,
    title: title,
    handle: handle,
    publication_ids: [],
    updated_at: None,
    description: None,
    description_html: description_html,
    image: None,
    sort_order: None,
    template_suffix: None,
    seo: seo,
    rule_set: None,
    products_count: None,
    is_smart: False,
    cursor: None,
    title_cursor: None,
    updated_at_cursor: None,
  )
}
