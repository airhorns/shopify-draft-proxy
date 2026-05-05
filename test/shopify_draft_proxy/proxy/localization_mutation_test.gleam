//// Mutation-path tests for `proxy/localization`.
////
//// Covers the five mutation roots — shopLocaleEnable/Update/Disable
//// and translationsRegister/Remove — including the userError envelope,
//// the staged_resource_ids signal, unknown-resource validation, and
//// captured source-content marker behavior while the Products domain is
//// absent from the current Gleam port state.

import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/localization
import shopify_draft_proxy/proxy/mutation_helpers
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{ShopLocaleRecord, TranslationRecord}

fn run_outcome(
  store_in: store.Store,
  document: String,
) -> mutation_helpers.MutationOutcome {
  let identity = synthetic_identity.new()
  let outcome =
    localization.process_mutation(
      store_in,
      identity,
      "/admin/api/2025-01/graphql.json",
      document,
      dict.new(),
    )
  outcome
}

fn run(store_in: store.Store, document: String) -> String {
  json.to_string(run_outcome(store_in, document).data)
}

fn seed_shop_locale(
  store_in: store.Store,
  locale: String,
  primary: Bool,
  published: Bool,
) -> store.Store {
  let #(_, s) =
    store.stage_shop_locale(
      store_in,
      ShopLocaleRecord(
        locale: locale,
        name: locale,
        primary: primary,
        published: published,
        market_web_presence_ids: [],
      ),
    )
  s
}

fn seed_source_content_marker(
  store_in: store.Store,
  resource_id: String,
  key: String,
  digest: String,
) -> store.Store {
  let #(_, s) =
    store.stage_translation(
      store_in,
      TranslationRecord(
        resource_id: resource_id,
        key: key,
        locale: "__source",
        value: "",
        translatable_content_digest: digest,
        market_id: None,
        updated_at: "1970-01-01T00:00:00Z",
        outdated: False,
      ),
    )
  s
}

fn repeated_translation_inputs(count: Int) -> String {
  repeat_translation_input_loop(count, [])
}

fn repeat_translation_input_loop(count: Int, acc: List(String)) -> String {
  case count {
    0 -> string.join(acc, ", ")
    _ ->
      repeat_translation_input_loop(count - 1, [
        "{ locale: \"fr\", key: \"title\", value: \"Bonjour\", translatableContentDigest: \"abc\" }",
        ..acc
      ])
  }
}

// ---------- envelope ----------

pub fn process_mutation_returns_data_envelope_test() {
  // shopLocaleEnable with a known iso code from the default catalog.
  let body =
    run(
      store.new(),
      "mutation { shopLocaleEnable(locale: \"fr\") { shopLocale { locale name } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"shopLocaleEnable\":{\"shopLocale\":{\"locale\":\"fr\",\"name\":\"French\"},\"userErrors\":[]}}}"
}

// ---------- shopLocaleEnable ----------

pub fn shop_locale_enable_creates_record_test() {
  let outcome =
    run_outcome(
      store.new(),
      "mutation { shopLocaleEnable(locale: \"ja\") { shopLocale { locale name primary published } userErrors { field } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"shopLocaleEnable\":{\"shopLocale\":{\"locale\":\"ja\",\"name\":\"Japanese\",\"primary\":false,\"published\":false},\"userErrors\":[]}}}"
  assert outcome.staged_resource_ids == ["ShopLocale/ja"]
  // The store now reflects the staged record.
  let assert Some(record) = store.get_effective_shop_locale(outcome.store, "ja")
  assert record.locale == "ja"
}

pub fn shop_locale_enable_unknown_locale_returns_user_error_test() {
  let body =
    run(
      store.new(),
      "mutation { shopLocaleEnable(locale: \"xx\") { shopLocale { locale } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"shopLocaleEnable\":{\"shopLocale\":null,\"userErrors\":[{\"field\":[\"locale\"],\"message\":\"The locale doesn't exist.\",\"code\":\"SHOP_LOCALE_DOES_NOT_EXIST\"}]}}}"
}

pub fn shop_locale_enable_primary_returns_user_error_test() {
  let body =
    run(
      store.new(),
      "mutation { shopLocaleEnable(locale: \"en\") { shopLocale { locale } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"shopLocaleEnable\":{\"shopLocale\":null,\"userErrors\":[{\"field\":[\"locale\"],\"message\":\"The primary locale of your store can't be changed through this endpoint.\",\"code\":\"CAN_NOT_MUTATE_PRIMARY_LOCALE\"}]}}}"
}

// ---------- shopLocaleUpdate ----------

pub fn shop_locale_update_modifies_published_test() {
  let s = seed_shop_locale(store.new(), "fr", False, False)
  let outcome =
    run_outcome(
      s,
      "mutation { shopLocaleUpdate(locale: \"fr\", shopLocale: { published: true }) { shopLocale { locale published } userErrors { field } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"shopLocaleUpdate\":{\"shopLocale\":{\"locale\":\"fr\",\"published\":true},\"userErrors\":[]}}}"
  let assert Some(record) = store.get_effective_shop_locale(outcome.store, "fr")
  assert record.published == True
}

pub fn shop_locale_update_unknown_locale_returns_user_error_test() {
  let body =
    run(
      store.new(),
      "mutation { shopLocaleUpdate(locale: \"de\", shopLocale: { published: true }) { shopLocale { locale } userErrors { field message code } } }",
    )
  // "de" is in the available catalog but not enabled, so update fails.
  assert body
    == "{\"data\":{\"shopLocaleUpdate\":{\"shopLocale\":null,\"userErrors\":[{\"field\":[\"locale\"],\"message\":\"The locale doesn't exist.\",\"code\":\"SHOP_LOCALE_DOES_NOT_EXIST\"}]}}}"
}

pub fn shop_locale_update_primary_unpublish_returns_user_error_test() {
  let body =
    run(
      store.new(),
      "mutation { shopLocaleUpdate(locale: \"en\", shopLocale: { published: false }) { shopLocale { locale } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"shopLocaleUpdate\":{\"shopLocale\":null,\"userErrors\":[{\"field\":[\"locale\"],\"message\":\"The primary locale of your store can't be changed through this endpoint.\",\"code\":\"CAN_NOT_MUTATE_PRIMARY_LOCALE\"}]}}}"
}

// ---------- shopLocaleDisable ----------

pub fn shop_locale_disable_removes_record_test() {
  let s = seed_shop_locale(store.new(), "fr", False, True)
  let outcome =
    run_outcome(
      s,
      "mutation { shopLocaleDisable(locale: \"fr\") { locale userErrors { field message } } }",
    )
  assert json.to_string(outcome.data)
    == "{\"data\":{\"shopLocaleDisable\":{\"locale\":\"fr\",\"userErrors\":[]}}}"
  // After disable the locale is no longer effective.
  assert store.get_effective_shop_locale(outcome.store, "fr") == None
}

pub fn shop_locale_disable_primary_returns_user_error_test() {
  // The default shop has "en" as primary — disabling it must fail.
  let body =
    run(
      store.new(),
      "mutation { shopLocaleDisable(locale: \"en\") { locale userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"shopLocaleDisable\":{\"locale\":null,\"userErrors\":[{\"field\":[\"locale\"],\"message\":\"The primary locale of your store can't be changed through this endpoint.\",\"code\":\"CAN_NOT_MUTATE_PRIMARY_LOCALE\"}]}}}"
}

pub fn shop_locale_disable_unknown_locale_returns_user_error_test() {
  let body =
    run(
      store.new(),
      "mutation { shopLocaleDisable(locale: \"de\") { locale userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"shopLocaleDisable\":{\"locale\":\"de\",\"userErrors\":[{\"field\":[\"locale\"],\"message\":\"The locale doesn't exist.\",\"code\":\"SHOP_LOCALE_DOES_NOT_EXIST\"}]}}}"
}

// ---------- translationsRegister ----------

pub fn translations_register_unknown_resource_returns_error_test() {
  // No Products domain and no seeded source marker → unknown ids fail.
  let s = seed_shop_locale(store.new(), "fr", False, True)
  let body =
    run(
      s,
      "mutation { translationsRegister(resourceId: \"gid://shopify/Product/1\", translations: [{ locale: \"fr\", key: \"title\", value: \"Bonjour\", translatableContentDigest: \"abc\" }]) { translations { key } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"translationsRegister\":{\"translations\":null,\"userErrors\":[{\"field\":[\"resourceId\"],\"message\":\"Resource gid://shopify/Product/1 does not exist\",\"code\":\"RESOURCE_NOT_FOUND\"}]}}}"
}

pub fn translations_register_against_seeded_source_marker_test() {
  let s =
    seed_shop_locale(store.new(), "fr", False, True)
    |> seed_source_content_marker("gid://shopify/Product/1", "title", "abc")
  let register =
    run_outcome(
      s,
      "mutation { translationsRegister(resourceId: \"gid://shopify/Product/1\", translations: [{ locale: \"fr\", key: \"title\", value: \"Bonjour\", translatableContentDigest: \"abc\" }]) { translations { key value locale outdated market { id } } userErrors { field message code } } }",
    )
  assert json.to_string(register.data)
    == "{\"data\":{\"translationsRegister\":{\"translations\":[{\"key\":\"title\",\"value\":\"Bonjour\",\"locale\":\"fr\",\"outdated\":false,\"market\":null}],\"userErrors\":[]}}}"

  let disabled =
    run_outcome(
      register.store,
      "mutation { shopLocaleDisable(locale: \"fr\") { locale userErrors { field message } } }",
    )
  let assert Ok(read_data) =
    localization.handle_localization_query(
      disabled.store,
      "{ translatableResource(resourceId: \"gid://shopify/Product/1\") { resourceId translations(locale: \"fr\") { key value locale } } }",
      dict.new(),
    )
  assert json.to_string(read_data)
    == "{\"translatableResource\":{\"resourceId\":\"gid://shopify/Product/1\",\"translations\":[]}}"
}

pub fn translations_register_accepts_market_id_and_read_filters_by_market_test() {
  let market_id = "gid://shopify/Market/123"
  let s =
    seed_shop_locale(store.new(), "es", False, True)
    |> seed_source_content_marker("gid://shopify/Product/1", "title", "abc")
  let register =
    run_outcome(
      s,
      "mutation { translationsRegister(resourceId: \"gid://shopify/Product/1\", translations: [{ locale: \"es\", key: \"title\", value: \"Hola\", marketId: \""
        <> market_id
        <> "\", translatableContentDigest: \"abc\" }]) { translations { key value locale outdated market { id __typename } } userErrors { field message code } } }",
    )

  assert json.to_string(register.data)
    == "{\"data\":{\"translationsRegister\":{\"translations\":[{\"key\":\"title\",\"value\":\"Hola\",\"locale\":\"es\",\"outdated\":false,\"market\":{\"id\":\"gid://shopify/Market/123\",\"__typename\":\"Market\"}}],\"userErrors\":[]}}}"

  let assert Ok(market_read_data) =
    localization.handle_localization_query(
      register.store,
      "{ translatableResource(resourceId: \"gid://shopify/Product/1\") { resourceId translations(locale: \"es\", marketId: \"gid://shopify/Market/123\") { key value locale market { id __typename } } } }",
      dict.new(),
    )
  assert json.to_string(market_read_data)
    == "{\"translatableResource\":{\"resourceId\":\"gid://shopify/Product/1\",\"translations\":[{\"key\":\"title\",\"value\":\"Hola\",\"locale\":\"es\",\"market\":{\"id\":\"gid://shopify/Market/123\",\"__typename\":\"Market\"}}]}}"

  let assert Ok(default_read_data) =
    localization.handle_localization_query(
      register.store,
      "{ translatableResource(resourceId: \"gid://shopify/Product/1\") { resourceId translations(locale: \"es\") { key value locale market { id } } } }",
      dict.new(),
    )
  assert json.to_string(default_read_data)
    == "{\"translatableResource\":{\"resourceId\":\"gid://shopify/Product/1\",\"translations\":[]}}"
}

pub fn translations_register_blank_resource_id_returns_error_test() {
  let body =
    run(
      store.new(),
      "mutation { translationsRegister(resourceId: \"\", translations: []) { translations { key } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"translationsRegister\":{\"translations\":null,\"userErrors\":[{\"field\":[\"resourceId\"],\"message\":\"Resource does not exist\",\"code\":\"RESOURCE_NOT_FOUND\"}]}}}"
}

pub fn translations_register_empty_list_noops_against_seeded_resource_test() {
  let s =
    seed_shop_locale(store.new(), "fr", False, True)
    |> seed_source_content_marker("gid://shopify/Product/1", "title", "abc")
  let body =
    run(
      s,
      "mutation { translationsRegister(resourceId: \"gid://shopify/Product/1\", translations: []) { translations { key } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"translationsRegister\":{\"translations\":[],\"userErrors\":[]}}}"
}

pub fn translations_register_blank_value_returns_resource_validation_error_test() {
  let s =
    seed_shop_locale(store.new(), "fr", False, True)
    |> seed_source_content_marker("gid://shopify/Product/1", "title", "abc")
  let body =
    run(
      s,
      "mutation { translationsRegister(resourceId: \"gid://shopify/Product/1\", translations: [{ locale: \"fr\", key: \"title\", value: \"\", translatableContentDigest: \"abc\" }]) { translations { key } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"translationsRegister\":{\"translations\":[],\"userErrors\":[{\"field\":[\"translations\",\"0\",\"value\"],\"message\":\"Value can't be blank\",\"code\":\"FAILS_RESOURCE_VALIDATION\"}]}}}"
}

pub fn translations_register_rejects_more_than_100_keys_test() {
  let s =
    seed_shop_locale(store.new(), "fr", False, True)
    |> seed_source_content_marker("gid://shopify/Product/1", "title", "abc")
  let body =
    run_outcome(
      s,
      "mutation { translationsRegister(resourceId: \"gid://shopify/Product/1\", translations: ["
        <> repeated_translation_inputs(101)
        <> "]) { translations { key } userErrors { field message code } } }",
    )
  assert json.to_string(body.data)
    == "{\"data\":{\"translationsRegister\":{\"translations\":null,\"userErrors\":[{\"field\":[\"resourceId\"],\"message\":\"Too many keys for resource - maximum 100 per mutation\",\"code\":\"TOO_MANY_KEYS_FOR_RESOURCE\"}]}}}"
  assert body.staged_resource_ids == []
}

// ---------- translationsRemove ----------

pub fn translations_remove_unknown_resource_returns_error_test() {
  let body =
    run(
      store.new(),
      "mutation { translationsRemove(resourceId: \"gid://shopify/Product/1\", translationKeys: [\"title\"], locales: [\"fr\"]) { translations { key } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"translationsRemove\":{\"translations\":null,\"userErrors\":[{\"field\":[\"resourceId\"],\"message\":\"Resource gid://shopify/Product/1 does not exist\",\"code\":\"RESOURCE_NOT_FOUND\"}]}}}"
}

pub fn translations_remove_blank_keys_returns_no_synthetic_blank_test() {
  let body =
    run(
      store.new(),
      "mutation { translationsRemove(resourceId: \"\", translationKeys: [], locales: []) { translations { key } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"translationsRemove\":{\"translations\":null,\"userErrors\":[{\"field\":[\"resourceId\"],\"message\":\"Resource does not exist\",\"code\":\"RESOURCE_NOT_FOUND\"}]}}}"
}

pub fn translations_remove_empty_locales_noops_against_seeded_resource_test() {
  let s =
    seed_shop_locale(store.new(), "fr", False, True)
    |> seed_source_content_marker("gid://shopify/Product/1", "title", "abc")
  let body =
    run(
      s,
      "mutation { translationsRemove(resourceId: \"gid://shopify/Product/1\", translationKeys: [\"title\"], locales: []) { translations { key } userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"translationsRemove\":{\"translations\":null,\"userErrors\":[]}}}"
}

pub fn translation_mutation_error_codes_are_translation_error_codes_test() {
  let allow_list = localization.translation_error_code_allow_list()
  let proxy_codes = localization.emitted_translation_mutation_error_codes()

  assert !list.contains(proxy_codes, "BLANK")
  assert list.all(proxy_codes, fn(code) { list.contains(allow_list, code) })
}

pub fn translations_remove_accepts_market_ids_and_clears_market_read_test() {
  let market_id = "gid://shopify/Market/123"
  let s =
    seed_shop_locale(store.new(), "es", False, True)
    |> seed_source_content_marker("gid://shopify/Product/1", "title", "abc")
  let register =
    run_outcome(
      s,
      "mutation { translationsRegister(resourceId: \"gid://shopify/Product/1\", translations: [{ locale: \"es\", key: \"title\", value: \"Hola\", marketId: \""
        <> market_id
        <> "\", translatableContentDigest: \"abc\" }]) { translations { key } userErrors { code } } }",
    )
  let remove =
    run_outcome(
      register.store,
      "mutation { translationsRemove(resourceId: \"gid://shopify/Product/1\", translationKeys: [\"title\"], locales: [\"es\"], marketIds: [\""
        <> market_id
        <> "\"]) { translations { key value locale market { id __typename } } userErrors { field message code } } }",
    )

  assert json.to_string(remove.data)
    == "{\"data\":{\"translationsRemove\":{\"translations\":[{\"key\":\"title\",\"value\":\"Hola\",\"locale\":\"es\",\"market\":{\"id\":\"gid://shopify/Market/123\",\"__typename\":\"Market\"}}],\"userErrors\":[]}}}"

  let assert Ok(read_data) =
    localization.handle_localization_query(
      remove.store,
      "{ translatableResource(resourceId: \"gid://shopify/Product/1\") { resourceId translations(locale: \"es\", marketId: \"gid://shopify/Market/123\") { key value locale market { id } } } }",
      dict.new(),
    )
  assert json.to_string(read_data)
    == "{\"translatableResource\":{\"resourceId\":\"gid://shopify/Product/1\",\"translations\":[]}}"
}
