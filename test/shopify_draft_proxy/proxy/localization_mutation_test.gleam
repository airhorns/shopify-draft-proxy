//// Mutation-path tests for `proxy/localization`.
////
//// Covers the five mutation roots — shopLocaleEnable/Update/Disable
//// and translationsRegister/Remove — including the userError envelope,
//// the staged_resource_ids signal, unknown-resource validation, and
//// captured source-content marker behavior while the Products domain is
//// absent from the current Gleam port state.

import gleam/dict
import gleam/json
import gleam/option.{None, Some}
import shopify_draft_proxy/proxy/localization
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/synthetic_identity
import shopify_draft_proxy/state/types.{ShopLocaleRecord, TranslationRecord}

fn run_outcome(
  store_in: store.Store,
  document: String,
) -> localization.MutationOutcome {
  let identity = synthetic_identity.new()
  let assert Ok(outcome) =
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
      "mutation { shopLocaleEnable(locale: \"xx\") { shopLocale { locale } userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"shopLocaleEnable\":{\"shopLocale\":null,\"userErrors\":[{\"field\":[\"locale\"],\"message\":\"Locale is invalid\"}]}}}"
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
      "mutation { shopLocaleUpdate(locale: \"de\", shopLocale: { published: true }) { shopLocale { locale } userErrors { field message } } }",
    )
  // "de" is in the available catalog but not enabled, so update fails.
  assert body
    == "{\"data\":{\"shopLocaleUpdate\":{\"shopLocale\":null,\"userErrors\":[{\"field\":[\"locale\"],\"message\":\"Locale is invalid\"}]}}}"
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
      "mutation { shopLocaleDisable(locale: \"en\") { locale userErrors { field message } } }",
    )
  assert body
    == "{\"data\":{\"shopLocaleDisable\":{\"locale\":\"en\",\"userErrors\":[{\"field\":[\"locale\"],\"message\":\"Locale is invalid\"}]}}}"
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

pub fn translations_register_blank_resource_id_returns_error_test() {
  let body =
    run(
      store.new(),
      "mutation { translationsRegister(resourceId: \"\", translations: []) { translations { key } userErrors { field message code } } }",
    )
  // Both the missing resource and the blank-translations error surface.
  assert body
    == "{\"data\":{\"translationsRegister\":{\"translations\":null,\"userErrors\":[{\"field\":[\"resourceId\"],\"message\":\"Resource does not exist\",\"code\":\"RESOURCE_NOT_FOUND\"},{\"field\":[\"translations\"],\"message\":\"At least one translation is required\",\"code\":\"BLANK\"}]}}}"
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

pub fn translations_remove_blank_keys_returns_error_test() {
  let body =
    run(
      store.new(),
      "mutation { translationsRemove(resourceId: \"\", translationKeys: [], locales: []) { translations { key } userErrors { field message code } } }",
    )
  // resource_not_found + blank keys + blank locales — three errors.
  assert body
    == "{\"data\":{\"translationsRemove\":{\"translations\":null,\"userErrors\":[{\"field\":[\"resourceId\"],\"message\":\"Resource does not exist\",\"code\":\"RESOURCE_NOT_FOUND\"},{\"field\":[\"translationKeys\"],\"message\":\"At least one translation key is required\",\"code\":\"BLANK\"},{\"field\":[\"locales\"],\"message\":\"At least one locale is required\",\"code\":\"BLANK\"}]}}}"
}
