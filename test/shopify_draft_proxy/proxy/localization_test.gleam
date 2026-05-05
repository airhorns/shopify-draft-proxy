//// Read-path tests for `proxy/localization`. Exercises the always-on
//// surfaces: availableLocales (default catalog), shopLocales (default
//// + custom + filter), translatableResource(s) (null/empty when no
//// Products + translations have been staged), and the predicate helpers.

import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/localization
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  LocaleRecord, ProductRecord, ProductSeoRecord, ShopLocaleRecord,
  TranslationRecord,
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
  // Default catalog is the eight ISO codes seeded in localization.gleam.
  assert result
    == "{\"availableLocales\":["
    <> "{\"isoCode\":\"en\",\"name\":\"English\"},"
    <> "{\"isoCode\":\"fr\",\"name\":\"French\"},"
    <> "{\"isoCode\":\"de\",\"name\":\"German\"},"
    <> "{\"isoCode\":\"es\",\"name\":\"Spanish\"},"
    <> "{\"isoCode\":\"it\",\"name\":\"Italian\"},"
    <> "{\"isoCode\":\"pt-BR\",\"name\":\"Portuguese (Brazil)\"},"
    <> "{\"isoCode\":\"ja\",\"name\":\"Japanese\"},"
    <> "{\"isoCode\":\"zh-CN\",\"name\":\"Chinese (Simplified)\"}"
    <> "]}"
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
