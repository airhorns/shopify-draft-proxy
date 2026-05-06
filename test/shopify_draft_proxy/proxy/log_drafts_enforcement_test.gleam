//// Enforcement test for the centralized mutation-logging contract.
////
//// Every domain module's `process_mutation` MUST populate
//// `MutationOutcome.log_drafts` so that `draft_proxy.route_mutation` can
//// record entries via `mutation_helpers.record_log_drafts`. The mandatory
//// `log_drafts: List(LogDraft)` field on every `MutationOutcome` is the
//// type-level contract; this test catches the runtime case where a handler
//// returns an empty list and silently drops the staged work from the log.
////
//// Without the log, Shopify Shell's `DraftBuffer` GenServer treats the
//// session as `:empty` even though state has been mutated, leaving
//// merchants with no way to commit or discard the work. See
//// `docs/bug-mutation-log-missing-domains.md`.
////
//// When you add a new mutation domain or root field, add a representative
//// happy-path mutation here. Failing this test means the dispatcher will
//// not record an entry for the new path.

import gleam/dict
import gleam/int
import gleam/list
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/state/store

fn enforce(name: String, body: String) {
  let proxy = draft_proxy.new()
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: body,
    )
  let #(Response(status: status, ..), after) =
    draft_proxy.process_request(proxy, request)
  let log_size = list.length(store.get_log(after.store))
  case status == 200 && log_size >= 1 {
    True -> Nil
    False ->
      panic as {
        "Domain "
        <> name
        <> " did not record a log entry for a happy-path mutation. status="
        <> int.to_string(status)
        <> " log_size="
        <> int.to_string(log_size)
        <> ". A new mutation handler may have forgotten to populate "
        <> "MutationOutcome.log_drafts. See docs/bug-mutation-log-missing-domains.md."
      }
  }
}

pub fn admin_platform_emits_log_draft_test() {
  enforce(
    "admin_platform",
    "{\"query\":\"mutation { backupRegionUpdate(region: { countryCode: CA }) { backupRegion { id } userErrors { message } } }\"}",
  )
}

pub fn apps_emits_log_draft_test() {
  enforce(
    "apps",
    "{\"query\":\"mutation { appUninstall { app { id } userErrors { message } } }\"}",
  )
}

pub fn bulk_operations_emits_log_draft_test() {
  enforce(
    "bulk_operations",
    "{\"query\":\"mutation { bulkOperationRunQuery(query: \\\"{ products { edges { node { id } } } }\\\", groupObjects: false) { bulkOperation { id } userErrors { message } } }\"}",
  )
}

pub fn functions_emits_log_draft_test() {
  enforce(
    "functions",
    "{\"query\":\"mutation { taxAppConfigure(ready: true) { taxAppConfiguration { id } userErrors { message } } }\"}",
  )
}

pub fn gift_cards_emits_log_draft_test() {
  enforce(
    "gift_cards",
    "{\"query\":\"mutation { giftCardCreate(input: { initialValue: { amount: \\\"5.00\\\", currencyCode: CAD } }) { giftCard { id } userErrors { message } } }\"}",
  )
}

pub fn localization_emits_log_draft_test() {
  enforce(
    "localization",
    "{\"query\":\"mutation { shopLocaleEnable(locale: \\\"fr\\\") { shopLocale { locale } userErrors { message } } }\"}",
  )
}

pub fn marketing_emits_log_draft_test() {
  enforce(
    "marketing",
    "{\"query\":\"mutation { marketingActivityCreateExternal(input: { title: \\\"Launch\\\", remoteId: \\\"remote-1\\\", remoteUrl: \\\"https://example.com/launch\\\", tactic: NEWSLETTER, marketingChannelType: EMAIL, urlParameterValue: \\\"utm_campaign=launch\\\", utm: { campaign: \\\"launch\\\", source: \\\"email\\\", medium: \\\"newsletter\\\" } }) { marketingActivity { id } userErrors { message } } }\"}",
  )
}

pub fn metafield_definitions_emits_log_draft_test() {
  enforce(
    "metafield_definitions",
    "{\"query\":\"mutation { standardMetafieldDefinitionEnable(ownerType: PRODUCT, id: \\\"gid://shopify/StandardMetafieldDefinitionTemplate/missing\\\") { createdDefinition { id } userErrors { message } } }\"}",
  )
}

pub fn saved_searches_emits_log_draft_test() {
  enforce(
    "saved_searches",
    "{\"query\":\"mutation { savedSearchCreate(input: { resourceType: ORDER, name: \\\"X\\\", query: \\\"tag:x\\\" }) { savedSearch { id } userErrors { message } } }\"}",
  )
}

pub fn segments_emits_log_draft_test() {
  enforce(
    "segments",
    "{\"query\":\"mutation { segmentCreate(name: \\\"VIPs\\\", query: \\\"number_of_orders >= 5\\\") { segment { id name } userErrors { field } } }\"}",
  )
}

pub fn webhooks_emits_log_draft_test() {
  enforce(
    "webhooks",
    "{\"query\":\"mutation { webhookSubscriptionCreate(topic: ORDERS_CREATE, webhookSubscription: { uri: \\\"https://hooks.example.com/orders\\\", format: JSON }) { webhookSubscription { id } userErrors { message } } }\"}",
  )
}
