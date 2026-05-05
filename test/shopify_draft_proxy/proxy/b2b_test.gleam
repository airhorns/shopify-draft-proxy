import gleam/dict
import gleam/int
import gleam/json
import gleam/list
import gleam/option.{type Option, None, Some}
import gleam/string
import shopify_draft_proxy/proxy/b2b
import shopify_draft_proxy/proxy/b2b_user_error_codes
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{
  type CustomerRecord, B2BCompanyRecord, CustomerRecord, ShopLocaleRecord,
  StorePropertyString,
}

fn graphql(proxy: draft_proxy.DraftProxy, query: String) {
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\"" <> escape(query) <> "\"}",
    )
  draft_proxy.process_request(proxy, request)
}

fn graphql_with_variables(
  proxy: draft_proxy.DraftProxy,
  query: String,
  variables: String,
) {
  let request =
    Request(
      method: "POST",
      path: "/admin/api/2026-04/graphql.json",
      headers: dict.new(),
      body: "{\"query\":\""
        <> escape(query)
        <> "\",\"variables\":"
        <> variables
        <> "}",
    )
  draft_proxy.process_request(proxy, request)
}

fn escape(value: String) -> String {
  value
  |> string.replace("\\", "\\\\")
  |> string.replace("\"", "\\\"")
}

fn proxy_with_primary_locale(locale: String) -> draft_proxy.DraftProxy {
  let proxy = draft_proxy.new()
  let #(_, seeded_store) =
    store.stage_shop_locale(
      proxy.store,
      ShopLocaleRecord(
        locale: locale,
        name: locale,
        primary: True,
        published: True,
        market_web_presence_ids: [],
      ),
    )
  proxy_state.DraftProxy(..proxy, store: seeded_store)
}

fn seed_customer(
  proxy: draft_proxy.DraftProxy,
  record: CustomerRecord,
) -> draft_proxy.DraftProxy {
  let #(_, seeded_store) = store.stage_create_customer(proxy.store, record)
  proxy_state.DraftProxy(..proxy, store: seeded_store)
}

fn customer(id: String, email: Option(String)) -> CustomerRecord {
  CustomerRecord(
    id: id,
    first_name: Some("Har"),
    last_name: Some("Buyer"),
    display_name: Some("Har Buyer"),
    email: email,
    legacy_resource_id: None,
    locale: Some("en"),
    note: None,
    can_delete: Some(True),
    verified_email: Some(True),
    data_sale_opt_out: False,
    tax_exempt: Some(False),
    tax_exemptions: [],
    state: Some("DISABLED"),
    tags: [],
    number_of_orders: Some("0"),
    amount_spent: None,
    default_email_address: None,
    default_phone_number: None,
    email_marketing_consent: None,
    sms_marketing_consent: None,
    default_address: None,
    account_activation_token: None,
    created_at: Some("2024-01-01T00:00:00.000Z"),
    updated_at: Some("2024-01-01T00:00:00.000Z"),
  )
}

fn contact_ids(count: Int) -> List(String) {
  contact_ids_loop(1, count, [])
}

fn contact_ids_loop(index: Int, count: Int, acc: List(String)) -> List(String) {
  case index > count {
    True -> list.reverse(acc)
    False ->
      contact_ids_loop(index + 1, count, [
        "gid://shopify/CompanyContact/" <> int.to_string(index),
        ..acc
      ])
  }
}

fn proxy_with_company_at_contact_cap(
  customer_record: CustomerRecord,
) -> draft_proxy.DraftProxy {
  let proxy = draft_proxy.new()
  let #(_, seeded_store) =
    store.stage_create_customer(proxy.store, customer_record)
  let company =
    B2BCompanyRecord(
      id: "gid://shopify/Company/606",
      cursor: None,
      data: dict.from_list([
        #("id", StorePropertyString("gid://shopify/Company/606")),
        #("name", StorePropertyString("HAR 606 Cap")),
      ]),
      contact_ids: contact_ids(10_000),
      main_contact_id: None,
      location_ids: [],
      contact_role_ids: [],
    )
  let #(_, seeded_store) =
    store.upsert_staged_b2b_company(seeded_store, company)
  proxy_state.DraftProxy(..proxy, store: seeded_store)
}

pub fn b2b_company_create_readback_and_log_test() {
  let proxy = draft_proxy.new()
  let create_query =
    "mutation { companyCreate(input: { company: { name: \"B2B Draft\", externalId: \"b2b-draft\" }, companyContact: { email: \"buyer@example.com\", firstName: \"B\", lastName: \"Buyer\", title: \"Lead\" }, companyLocation: { name: \"B2B HQ\", phone: \"+16135550199\", locale: \"en\" } }) { company { id name externalId contactsCount { count } locationsCount { count } mainContact { id title isMainContact customer { id email } } locations(first: 5) { nodes { id name phone } } contactRoles(first: 5) { nodes { id name } } } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy, create_query)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(
    create_json,
    "\"id\":\"gid://shopify/Company/1?shopify-draft-proxy=synthetic\"",
  )
  assert string.contains(create_json, "\"name\":\"B2B Draft\"")
  assert string.contains(create_json, "\"contactsCount\":{\"count\":1}")
  assert string.contains(create_json, "\"locationsCount\":{\"count\":1}")
  assert string.contains(create_json, "\"isMainContact\":true")
  assert string.contains(create_json, "\"email\":\"buyer@example.com\"")

  let read_query =
    "query { company(id: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\") { id name externalId contactsCount { count } locations(first: 5) { nodes { id name phone } } contactRoles(first: 5) { nodes { id name } } } companies(first: 5) { nodes { id name } } companiesCount { count precision } }"
  let #(Response(status: read_status, body: read_body, ..), proxy) =
    graphql(proxy, read_query)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"companiesCount\":{\"count\":1")
  assert string.contains(read_json, "\"externalId\":\"b2b-draft\"")
  assert string.contains(read_json, "\"phone\":\"+16135550199\"")
  assert string.contains(read_json, "\"Location admin\"")
  assert string.contains(read_json, "\"Ordering only\"")

  let log_request =
    Request(method: "GET", path: "/__meta/log", headers: dict.new(), body: "")
  let #(Response(status: log_status, body: log_body, ..), _) =
    draft_proxy.process_request(proxy, log_request)
  assert log_status == 200
  let log_json = json.to_string(log_body)
  assert string.contains(log_json, "\"domain\":\"b2b\"")
  assert string.contains(
    log_json,
    "\"query\":\"" <> escape(create_query) <> "\"",
  )
}

pub fn b2b_company_name_validation_and_sanitization_test() {
  let long_name = string.repeat("x", times: 300)
  let #(Response(status: long_status, body: long_body, ..), _) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \""
        <> long_name
        <> "\" } }) { company { id name } userErrors { field message code } } }",
    )
  assert long_status == 200
  let long_json = json.to_string(long_body)
  assert string.contains(long_json, "\"company\":null")
  assert string.contains(
    long_json,
    "\"field\":[\"input\",\"company\",\"name\"]",
  )
  assert string.contains(long_json, "\"code\":\"TOO_LONG\"")

  let #(Response(status: html_status, body: html_body, ..), _) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"<b>B2B Draft</b>\" } }) { company { id name } userErrors { field message code } } }",
    )
  assert html_status == 200
  let html_json = json.to_string(html_body)
  assert string.contains(html_json, "\"name\":\"B2B Draft\"")
  assert string.contains(html_json, "\"userErrors\":[]")
  assert !string.contains(html_json, "<b>")
}

pub fn b2b_company_create_rejects_external_id_validation_and_duplicate_test() {
  let long_external_id = string.repeat("x", times: 65)
  let #(Response(status: long_status, body: long_body, ..), _) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Draft\", externalId: \""
        <> long_external_id
        <> "\" } }) { company { id externalId } userErrors { field message code detail } } }",
    )
  assert long_status == 200
  let long_json = json.to_string(long_body)
  assert string.contains(long_json, "\"company\":null")
  assert string.contains(
    long_json,
    "\"field\":[\"input\",\"company\",\"externalId\"]",
  )
  assert string.contains(long_json, "\"code\":\"TOO_LONG\"")

  let #(Response(status: invalid_status, body: invalid_body, ..), _) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Draft\", externalId: \"has spaces\" } }) { company { id externalId } userErrors { field message code detail } } }",
    )
  assert invalid_status == 200
  let invalid_json = json.to_string(invalid_body)
  assert string.contains(invalid_json, "\"company\":null")
  assert string.contains(invalid_json, "\"code\":\"INVALID\"")
  assert string.contains(
    invalid_json,
    "\"detail\":\"external_id_contains_invalid_chars\"",
  )

  let create_query =
    "mutation { companyCreate(input: { company: { name: \"Duplicate One\", externalId: \"ACME-1\" } }) { company { id externalId } userErrors { field message code } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(draft_proxy.new(), create_query)
  assert first_status == 200
  assert string.contains(json.to_string(first_body), "\"userErrors\":[]")

  let #(Response(status: duplicate_status, body: duplicate_body, ..), _) =
    graphql(
      proxy,
      "mutation { companyCreate(input: { company: { name: \"Duplicate Two\", externalId: \"ACME-1\" } }) { company { id externalId } userErrors { field message code } } }",
    )
  assert duplicate_status == 200
  let duplicate_json = json.to_string(duplicate_body)
  assert string.contains(duplicate_json, "\"company\":null")
  assert string.contains(duplicate_json, "\"code\":\"TAKEN\"")
}

pub fn b2b_company_update_validates_external_id_and_duplicate_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: first_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyCreate(input: { company: { name: \"First\", externalId: \"ACME-1\" } }) { company { id } userErrors { code } } }",
    )
  assert first_status == 200
  let #(Response(status: second_status, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyCreate(input: { company: { name: \"Second\", externalId: \"ACME-2\" } }) { company { id } userErrors { code } } }",
    )
  assert second_status == 200

  let #(Response(status: self_status, body: self_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyUpdate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { externalId: \"ACME-1\" }) { company { id externalId } userErrors { field message code } } }",
    )
  assert self_status == 200
  assert string.contains(json.to_string(self_body), "\"userErrors\":[]")

  let #(Response(status: duplicate_status, body: duplicate_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyUpdate(companyId: \"gid://shopify/Company/6?shopify-draft-proxy=synthetic\", input: { externalId: \"ACME-1\" }) { company { id externalId } userErrors { field message code } } }",
    )
  assert duplicate_status == 200
  let duplicate_json = json.to_string(duplicate_body)
  assert string.contains(duplicate_json, "\"company\":null")
  assert string.contains(duplicate_json, "\"code\":\"TAKEN\"")

  let long_external_id = string.repeat("x", times: 65)
  let #(Response(status: long_status, body: long_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyUpdate(companyId: \"gid://shopify/Company/6?shopify-draft-proxy=synthetic\", input: { externalId: \""
        <> long_external_id
        <> "\" }) { company { id externalId } userErrors { field message code } } }",
    )
  assert long_status == 200
  assert string.contains(json.to_string(long_body), "\"code\":\"TOO_LONG\"")

  let #(Response(status: invalid_status, body: invalid_body, ..), _) =
    graphql(
      proxy,
      "mutation { companyUpdate(companyId: \"gid://shopify/Company/6?shopify-draft-proxy=synthetic\", input: { externalId: \"bad id\" }) { company { id externalId } userErrors { field message code detail } } }",
    )
  assert invalid_status == 200
  let invalid_json = json.to_string(invalid_body)
  assert string.contains(invalid_json, "\"code\":\"INVALID\"")
  assert string.contains(
    invalid_json,
    "\"detail\":\"external_id_contains_invalid_chars\"",
  )
}

pub fn b2b_company_update_rejects_note_html_and_too_long_test() {
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Draft\" } }) { company { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let invalid_note =
    "<script>" <> string.repeat("x", times: 6000) <> "</script>"
  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql(
      proxy,
      "mutation { companyUpdate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { note: \""
        <> invalid_note
        <> "\" }) { company { id note } userErrors { field message code } } }",
    )
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"company\":null")
  assert string.contains(update_json, "\"field\":[\"input\",\"notes\"]")
  assert string.contains(update_json, "\"code\":\"CONTAINS_HTML_TAGS\"")
  assert string.contains(update_json, "\"code\":\"TOO_LONG\"")
}

pub fn b2b_contact_create_rejects_title_and_notes_validation_test() {
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Draft\" } }) { company { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let invalid_title = "<b>" <> string.repeat("x", times: 260) <> "</b>"
  let invalid_notes = "<i>" <> string.repeat("n", times: 5001) <> "</i>"
  let #(Response(status: contact_status, body: contact_body, ..), _) =
    graphql(
      proxy,
      "mutation { companyContactCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { email: \"buyer@example.com\", title: \""
        <> invalid_title
        <> "\", notes: \""
        <> invalid_notes
        <> "\" }) { companyContact { id title } userErrors { field message code } } }",
    )
  assert contact_status == 200
  let contact_json = json.to_string(contact_body)
  assert string.contains(contact_json, "\"companyContact\":null")
  assert string.contains(contact_json, "\"field\":[\"input\",\"title\"]")
  assert string.contains(contact_json, "\"field\":[\"input\",\"notes\"]")
  assert string.contains(contact_json, "\"code\":\"CONTAINS_HTML_TAGS\"")
  assert string.contains(contact_json, "\"code\":\"TOO_LONG\"")
}

pub fn b2b_location_create_rejects_name_and_note_validation_test() {
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Draft\" } }) { company { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let long_name = string.repeat("x", times: 300)
  let invalid_note =
    "<script>" <> string.repeat("x", times: 6000) <> "</script>"
  let #(Response(status: location_status, body: location_body, ..), _) =
    graphql(
      proxy,
      "mutation { companyLocationCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { name: \""
        <> long_name
        <> "\", note: \""
        <> invalid_note
        <> "\" }) { companyLocation { id name note } userErrors { field message code } } }",
    )
  assert location_status == 200
  let location_json = json.to_string(location_body)
  assert string.contains(location_json, "\"companyLocation\":null")
  assert string.contains(location_json, "\"field\":[\"input\",\"name\"]")
  assert string.contains(location_json, "\"field\":[\"input\",\"notes\"]")
  assert string.contains(location_json, "\"code\":\"CONTAINS_HTML_TAGS\"")
  assert string.contains(location_json, "\"code\":\"TOO_LONG\"")
}

pub fn b2b_location_create_and_update_validates_external_id_test() {
  let proxy = draft_proxy.new()
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyCreate(input: { company: { name: \"B2B Draft\" }, companyLocation: { name: \"HQ\", externalId: \"LOC-1\" } }) { company { id locations(first: 5) { nodes { id externalId } } } userErrors { code } } }",
    )
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let long_external_id = string.repeat("x", times: 65)
  let #(Response(status: long_status, body: long_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { name: \"Long\", externalId: \""
        <> long_external_id
        <> "\" }) { companyLocation { id externalId } userErrors { field message code } } }",
    )
  assert long_status == 200
  assert string.contains(json.to_string(long_body), "\"code\":\"TOO_LONG\"")

  let #(Response(status: invalid_status, body: invalid_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { name: \"Invalid\", externalId: \"bad id\" }) { companyLocation { id externalId } userErrors { field message code detail } } }",
    )
  assert invalid_status == 200
  let invalid_json = json.to_string(invalid_body)
  assert string.contains(invalid_json, "\"code\":\"INVALID\"")
  assert string.contains(
    invalid_json,
    "\"detail\":\"external_id_contains_invalid_chars\"",
  )

  let #(Response(status: duplicate_status, body: duplicate_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { name: \"Duplicate\", externalId: \"LOC-1\" }) { companyLocation { id externalId } userErrors { field message code } } }",
    )
  assert duplicate_status == 200
  let duplicate_json = json.to_string(duplicate_body)
  assert string.contains(duplicate_json, "\"companyLocation\":null")
  assert string.contains(duplicate_json, "\"code\":\"TAKEN\"")

  let #(Response(status: self_status, body: self_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationUpdate(companyLocationId: \"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\", input: { externalId: \"LOC-1\" }) { companyLocation { id externalId } userErrors { field message code } } }",
    )
  assert self_status == 200
  assert string.contains(json.to_string(self_body), "\"userErrors\":[]")

  let #(Response(status: second_status, body: second_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { name: \"Second\", externalId: \"LOC-2\" }) { companyLocation { id externalId } userErrors { field message code } } }",
    )
  assert second_status == 200
  assert string.contains(json.to_string(second_body), "\"userErrors\":[]")

  let #(Response(status: update_status, body: update_body, ..), _) =
    graphql(
      proxy,
      "mutation { companyLocationUpdate(companyLocationId: \"gid://shopify/CompanyLocation/10?shopify-draft-proxy=synthetic\", input: { externalId: \"LOC-1\" }) { companyLocation { id externalId } userErrors { field message code } } }",
    )
  assert update_status == 200
  let update_json = json.to_string(update_body)
  assert string.contains(update_json, "\"companyLocation\":null")
  assert string.contains(update_json, "\"code\":\"TAKEN\"")
}

pub fn b2b_company_create_rejects_location_billing_and_tax_guardrails_test() {
  let #(Response(status: conflict_status, body: conflict_body, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Billing Conflict\" }, companyLocation: { name: \"HQ\", billingSameAsShipping: true, shippingAddress: { address1: \"Ship\" }, billingAddress: { address1: \"Bill\" } } }) { company { id } userErrors { field message code } } }",
    )
  assert conflict_status == 200
  let conflict_json = json.to_string(conflict_body)
  assert string.contains(conflict_json, "\"company\":null")
  assert string.contains(
    conflict_json,
    "\"field\":[\"input\",\"companyLocation\",\"billingAddress\"]",
  )
  assert string.contains(conflict_json, "\"code\":\"INVALID_INPUT\"")

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { companies(first: 5) { nodes { id } } companiesCount { count } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"nodes\":[]")
  assert string.contains(read_json, "\"count\":0")

  let #(Response(status: missing_status, body: missing_body, ..), _) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Missing Billing\" }, companyLocation: { name: \"HQ\", billingSameAsShipping: false } }) { company { id } userErrors { field message code } } }",
    )
  assert missing_status == 200
  let missing_json = json.to_string(missing_body)
  assert string.contains(missing_json, "\"company\":null")
  assert string.contains(
    missing_json,
    "\"field\":[\"input\",\"companyLocation\",\"billingAddress\"]",
  )
  assert string.contains(missing_json, "\"code\":\"INVALID_INPUT\"")

  let #(Response(status: tax_status, body: tax_body, ..), _) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Null Tax\" }, companyLocation: { name: \"HQ\", taxExempt: null } }) { company { id } userErrors { field message code } } }",
    )
  assert tax_status == 200
  let tax_json = json.to_string(tax_body)
  assert string.contains(tax_json, "\"company\":null")
  assert string.contains(
    tax_json,
    "\"field\":[\"input\",\"companyLocation\",\"taxExempt\"]",
  )
  assert string.contains(tax_json, "\"code\":\"INVALID_INPUT\"")
}

pub fn b2b_location_create_rejects_billing_and_tax_guardrails_test() {
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Draft\" }, companyLocation: { name: \"HQ\" } }) { company { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: conflict_status, body: conflict_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { name: \"Conflict\", billingSameAsShipping: true, shippingAddress: { address1: \"Ship\" }, billingAddress: { address1: \"Bill\" } }) { companyLocation { id } userErrors { field message code } } }",
    )
  assert conflict_status == 200
  let conflict_json = json.to_string(conflict_body)
  assert string.contains(conflict_json, "\"companyLocation\":null")
  assert string.contains(
    conflict_json,
    "\"field\":[\"input\",\"billingAddress\"]",
  )
  assert string.contains(conflict_json, "\"code\":\"INVALID_INPUT\"")

  let #(Response(status: missing_status, body: missing_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { name: \"Missing Billing\", billingSameAsShipping: false }) { companyLocation { id } userErrors { field message code } } }",
    )
  assert missing_status == 200
  let missing_json = json.to_string(missing_body)
  assert string.contains(missing_json, "\"companyLocation\":null")
  assert string.contains(
    missing_json,
    "\"field\":[\"input\",\"billingAddress\"]",
  )
  assert string.contains(missing_json, "\"code\":\"INVALID_INPUT\"")

  let #(Response(status: tax_status, body: tax_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { name: \"Null Tax\", taxExempt: null }) { companyLocation { id } userErrors { field message code } } }",
    )
  assert tax_status == 200
  let tax_json = json.to_string(tax_body)
  assert string.contains(tax_json, "\"companyLocation\":null")
  assert string.contains(tax_json, "\"field\":[\"input\",\"taxExempt\"]")
  assert string.contains(tax_json, "\"code\":\"INVALID_INPUT\"")

  let read_query =
    "query { company(id: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\") { locations(first: 5) { nodes { id name } } locationsCount: locations(first: 5) { nodes { id } } } companiesCount { count } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(
    read_json,
    "\"nodes\":[{\"id\":\"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\",\"name\":\"HQ\"}]",
  )
  assert !string.contains(read_json, "Conflict")
  assert !string.contains(read_json, "Missing Billing")
  assert !string.contains(read_json, "Null Tax")
}

pub fn b2b_location_update_rejects_billing_and_tax_guardrails_test() {
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Draft\" }, companyLocation: { name: \"HQ\" } }) { company { id locations(first: 5) { nodes { id name } } } userErrors { field message code } } }",
    )
  assert create_status == 200

  let location_id =
    "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic"
  let #(Response(status: conflict_status, body: conflict_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationUpdate(companyLocationId: \""
        <> location_id
        <> "\", input: { name: \"Conflict\", billingSameAsShipping: true, billingAddress: { address1: \"Bill\" } }) { companyLocation { id name } userErrors { field message code } } }",
    )
  assert conflict_status == 200
  let conflict_json = json.to_string(conflict_body)
  assert string.contains(conflict_json, "\"companyLocation\":null")
  assert string.contains(
    conflict_json,
    "\"field\":[\"input\",\"billingAddress\"]",
  )
  assert string.contains(conflict_json, "\"code\":\"INVALID_INPUT\"")

  let #(Response(status: missing_status, body: missing_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationUpdate(companyLocationId: \""
        <> location_id
        <> "\", input: { name: \"Missing Billing\", billingSameAsShipping: false }) { companyLocation { id name } userErrors { field message code } } }",
    )
  assert missing_status == 200
  let missing_json = json.to_string(missing_body)
  assert string.contains(missing_json, "\"companyLocation\":null")
  assert string.contains(
    missing_json,
    "\"field\":[\"input\",\"billingAddress\"]",
  )
  assert string.contains(missing_json, "\"code\":\"INVALID_INPUT\"")

  let #(Response(status: tax_status, body: tax_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationUpdate(companyLocationId: \""
        <> location_id
        <> "\", input: { name: \"Null Tax\", taxExempt: null }) { companyLocation { id name } userErrors { field message code } } }",
    )
  assert tax_status == 200
  let tax_json = json.to_string(tax_body)
  assert string.contains(tax_json, "\"companyLocation\":null")
  assert string.contains(tax_json, "\"field\":[\"input\",\"taxExempt\"]")
  assert string.contains(tax_json, "\"code\":\"INVALID_INPUT\"")

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { companyLocation(id: \""
        <> location_id
        <> "\") { id name billingAddress { address1 } taxSettings { taxExempt } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"name\":\"HQ\"")
  assert !string.contains(read_json, "Conflict")
  assert !string.contains(read_json, "Missing Billing")
  assert !string.contains(read_json, "Null Tax")
}

pub fn b2b_tax_settings_update_validates_required_and_nullable_args_test() {
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Tax\" }, companyLocation: { name: \"HQ\" } }) { company { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let location_id =
    "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic"
  let #(Response(status: empty_status, body: empty_body, ..), proxy) =
    graphql_with_variables(
      proxy,
      "mutation TaxEmpty($locationId: ID!) { companyLocationTaxSettingsUpdate(companyLocationId: $locationId) { companyLocation { id taxSettings { taxExempt taxExemptions } } userErrors { field message code } } }",
      "{\"locationId\":\"" <> location_id <> "\"}",
    )
  assert empty_status == 200
  let empty_json = json.to_string(empty_body)
  assert string.contains(empty_json, "\"companyLocation\":null")
  assert string.contains(empty_json, "\"field\":[\"companyLocationId\"]")
  assert string.contains(empty_json, "\"code\":\"NO_INPUT\"")

  let #(Response(status: null_status, body: null_body, ..), _) =
    graphql_with_variables(
      proxy,
      "mutation TaxNull($locationId: ID!, $taxExempt: Boolean) { companyLocationTaxSettingsUpdate(companyLocationId: $locationId, taxExempt: $taxExempt) { companyLocation { id taxSettings { taxExempt } } userErrors { field message code } } }",
      "{\"locationId\":\"" <> location_id <> "\",\"taxExempt\":null}",
    )
  assert null_status == 200
  let null_json = json.to_string(null_body)
  assert string.contains(null_json, "\"companyLocation\":null")
  assert string.contains(null_json, "\"field\":[\"taxExempt\"]")
  assert string.contains(null_json, "\"code\":\"INVALID_INPUT\"")
}

pub fn b2b_tax_settings_update_rejects_invalid_tax_exemption_enum_test() {
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Tax\" }, companyLocation: { name: \"HQ\" } }) { company { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let location_id =
    "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic"
  let #(Response(status: literal_status, body: literal_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationTaxSettingsUpdate(companyLocationId: \""
        <> location_id
        <> "\", exemptionsToAssign: [\"NOT_A_REAL_EXEMPTION\"]) { companyLocation { id taxSettings { taxExemptions } } userErrors { field message code } } }",
    )
  assert literal_status == 200
  let literal_json = json.to_string(literal_body)
  assert string.contains(literal_json, "\"errors\"")
  assert string.contains(
    literal_json,
    "\"code\":\"argumentLiteralsIncompatible\"",
  )
  assert string.contains(literal_json, "NOT_A_REAL_EXEMPTION")
  assert string.contains(literal_json, "CA_STATUS_CARD_EXEMPTION")
  assert !string.contains(literal_json, "\"taxExemptions\"")

  let #(Response(status: invalid_status, body: invalid_body, ..), _) =
    graphql_with_variables(
      proxy,
      "mutation TaxInvalid($locationId: ID!, $exemptionsToAssign: [TaxExemption!]) { companyLocationTaxSettingsUpdate(companyLocationId: $locationId, exemptionsToAssign: $exemptionsToAssign) { companyLocation { id taxSettings { taxExemptions } } userErrors { field message code } } }",
      "{\"locationId\":\""
        <> location_id
        <> "\",\"exemptionsToAssign\":[\"NOT_A_REAL_EXEMPTION\"]}",
    )
  assert invalid_status == 200
  let invalid_json = json.to_string(invalid_body)
  assert string.contains(invalid_json, "\"errors\"")
  assert string.contains(invalid_json, "\"code\":\"INVALID_VARIABLE\"")
  assert string.contains(invalid_json, "NOT_A_REAL_EXEMPTION")
  assert string.contains(invalid_json, "CA_STATUS_CARD_EXEMPTION")
}

pub fn b2b_tax_settings_update_stages_valid_assign_and_remove_test() {
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"B2B Tax\" }, companyLocation: { name: \"HQ\" } }) { company { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let location_id =
    "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic"
  let #(Response(status: assign_status, body: assign_body, ..), proxy) =
    graphql_with_variables(
      proxy,
      "mutation TaxAssign($locationId: ID!, $assign: [TaxExemption!]) { companyLocationTaxSettingsUpdate(companyLocationId: $locationId, exemptionsToAssign: $assign) { companyLocation { id taxSettings { taxExemptions } } userErrors { field message code } } }",
      "{\"locationId\":\""
        <> location_id
        <> "\",\"assign\":[\"CA_BC_RESELLER_EXEMPTION\",\"US_CA_RESELLER_EXEMPTION\"]}",
    )
  assert assign_status == 200
  let assign_json = json.to_string(assign_body)
  assert string.contains(assign_json, "\"userErrors\":[]")
  assert string.contains(assign_json, "CA_BC_RESELLER_EXEMPTION")
  assert string.contains(assign_json, "US_CA_RESELLER_EXEMPTION")

  let #(Response(status: remove_status, body: remove_body, ..), _) =
    graphql_with_variables(
      proxy,
      "mutation TaxRemove($locationId: ID!, $remove: [TaxExemption!]) { companyLocationTaxSettingsUpdate(companyLocationId: $locationId, exemptionsToRemove: $remove) { companyLocation { id taxSettings { taxExemptions } } userErrors { field message code } } }",
      "{\"locationId\":\""
        <> location_id
        <> "\",\"remove\":[\"CA_BC_RESELLER_EXEMPTION\"]}",
    )
  assert remove_status == 200
  let remove_json = json.to_string(remove_body)
  assert string.contains(remove_json, "\"userErrors\":[]")
  assert !string.contains(remove_json, "CA_BC_RESELLER_EXEMPTION")
  assert string.contains(remove_json, "US_CA_RESELLER_EXEMPTION")
}

pub fn b2b_location_create_uses_shipping_address1_name_fallback_test() {
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"Acme\" } }) { company { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: location_status, body: location_body, ..), _) =
    graphql(
      proxy,
      "mutation { companyLocationCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { shippingAddress: { address1: \"123 Main\" } }) { companyLocation { id name shippingAddress { address1 } } userErrors { field message code } } }",
    )
  assert location_status == 200
  let location_json = json.to_string(location_body)
  assert string.contains(location_json, "\"userErrors\":[]")
  assert string.contains(location_json, "\"name\":\"123 Main\"")
  assert string.contains(location_json, "\"address1\":\"123 Main\"")
}

pub fn b2b_location_assign_address_rejects_duplicate_address_types_test() {
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"Acme\" }, companyLocation: { name: \"HQ\" } }) { company { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: assign_status, body: assign_body, ..), _) =
    graphql(
      proxy,
      "mutation { companyLocationAssignAddress(locationId: \"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\", address: { address1: \"123 Main\" }, addressTypes: [BILLING, BILLING]) { addresses { id address1 } userErrors { field message code } } }",
    )
  assert assign_status == 200
  let assign_json = json.to_string(assign_body)
  assert string.contains(assign_json, "\"addresses\":null")
  assert string.contains(assign_json, "\"field\":null")
  assert string.contains(assign_json, "\"message\":\"Invalid input.\"")
  assert string.contains(assign_json, "\"code\":\"INVALID_INPUT\"")
}

pub fn b2b_address_delete_clears_shared_billing_shipping_anchor_test() {
  let #(Response(status: create_status, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"Acme\" } }) { company { id } userErrors { field message code } } }",
    )
  assert create_status == 200

  let #(Response(status: location_status, body: location_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { name: \"Shared\", billingSameAsShipping: true, shippingAddress: { address1: \"123 Main\" } }) { companyLocation { id billingSameAsShipping billingAddress { id address1 } shippingAddress { id address1 } } userErrors { field message code } } }",
    )
  assert location_status == 200
  let location_json = json.to_string(location_body)
  assert string.contains(location_json, "\"billingSameAsShipping\":true")
  assert string.contains(
    location_json,
    "\"id\":\"gid://shopify/CompanyAddress/7?shopify-draft-proxy=synthetic\"",
  )

  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyAddressDelete(addressId: \"gid://shopify/CompanyAddress/7?shopify-draft-proxy=synthetic\") { deletedAddressId userErrors { field message code } } }",
    )
  assert delete_status == 200
  assert string.contains(json.to_string(delete_body), "\"userErrors\":[]")

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { companyLocation(id: \"gid://shopify/CompanyLocation/6?shopify-draft-proxy=synthetic\") { id billingSameAsShipping billingAddress { id } shippingAddress { id } } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"billingSameAsShipping\":false")
  assert string.contains(read_json, "\"billingAddress\":null")
  assert string.contains(read_json, "\"shippingAddress\":null")
}

pub fn b2b_location_delete_cascades_contact_role_assignments_test() {
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(
      draft_proxy.new(),
      "mutation { companyCreate(input: { company: { name: \"Acme\" }, companyContact: { email: \"buyer@example.com\" }, companyLocation: { name: \"HQ\" } }) { company { contacts(first: 5) { nodes { id roleAssignments(first: 5) { nodes { id companyLocation { id } } } } } } userErrors { field message code } } }",
    )
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"userErrors\":[]")
  assert string.contains(
    create_json,
    "\"id\":\"gid://shopify/CompanyContactRoleAssignment/7?shopify-draft-proxy=synthetic\"",
  )
  assert string.contains(
    create_json,
    "\"companyLocation\":{\"id\":\"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\"}",
  )

  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql(
      proxy,
      "mutation { companyLocationDelete(companyLocationId: \"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\") { deletedCompanyLocationId userErrors { field message code } } }",
    )
  assert delete_status == 200
  assert string.contains(json.to_string(delete_body), "\"userErrors\":[]")

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { companyContact(id: \"gid://shopify/CompanyContact/5?shopify-draft-proxy=synthetic\") { id roleAssignments(first: 5) { nodes { id companyLocation { id } } } } companyLocation(id: \"gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic\") { id } }",
    )
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"companyLocation\":null")
  assert string.contains(read_json, "\"roleAssignments\":{\"nodes\":[]}")
}

pub fn b2b_email_delivery_root_is_not_local_support_test() {
  assert !b2b.is_b2b_mutation_root("companyContactSendWelcomeEmail")
  let #(Response(status: status, body: body, ..), _) =
    graphql(
      draft_proxy.new(),
      "mutation { companyContactSendWelcomeEmail(companyContactId: \"gid://shopify/CompanyContact/1\") { userErrors { message code } } }",
    )
  assert status == 400
  assert string.contains(
    json.to_string(body),
    "No mutation dispatcher implemented for root field: companyContactSendWelcomeEmail",
  )
}

pub fn b2b_contact_create_normalizes_phone_locale_and_notes_test() {
  let proxy = proxy_with_primary_locale("fr-CA")
  let create_company =
    "mutation { companyCreate(input: { company: { name: \"HAR 614\" }, companyLocation: { name: \"HQ\" } }) { company { id } userErrors { code } } }"
  let #(Response(status: company_status, body: company_body, ..), proxy) =
    graphql(proxy, create_company)
  assert company_status == 200
  assert string.contains(json.to_string(company_body), "\"userErrors\":[]")

  let create_contact =
    "mutation { companyContactCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { email: \"buyer614@example.com\", phone: \"(415) 555-1234\", note: \"Safe note\" }) { companyContact { id email phone locale notes note } userErrors { field message code } } }"
  let #(Response(status: contact_status, body: contact_body, ..), proxy) =
    graphql(proxy, create_contact)
  assert contact_status == 200
  let contact_json = json.to_string(contact_body)
  assert string.contains(contact_json, "\"phone\":\"+14155551234\"")
  assert string.contains(contact_json, "\"locale\":\"fr-CA\"")
  assert string.contains(contact_json, "\"notes\":\"Safe note\"")
  assert string.contains(contact_json, "\"note\":\"Safe note\"")
  assert string.contains(contact_json, "\"userErrors\":[]")

  let read_contact =
    "query { company(id: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\") { contacts(first: 5) { nodes { phone locale notes note } } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_contact)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"phone\":\"+14155551234\"")
  assert string.contains(read_json, "\"locale\":\"fr-CA\"")
  assert string.contains(read_json, "\"notes\":\"Safe note\"")
  assert string.contains(read_json, "\"note\":\"Safe note\"")
}

pub fn b2b_contact_create_rejects_invalid_phone_locale_and_html_notes_test() {
  let proxy = draft_proxy.new()
  let create_company =
    "mutation { companyCreate(input: { company: { name: \"HAR 614\" }, companyLocation: { name: \"HQ\" } }) { company { id } userErrors { code } } }"
  let #(Response(status: company_status, body: company_body, ..), proxy) =
    graphql(proxy, create_company)
  assert company_status == 200
  assert string.contains(json.to_string(company_body), "\"userErrors\":[]")

  let create_contact =
    "mutation { companyContactCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { email: \"buyer614-invalid@example.com\", phone: \"not-a-phone\", locale: \"not_a_locale\", note: \"<script>x</script>\" }) { companyContact { id } userErrors { field message code } } }"
  let #(Response(status: contact_status, body: contact_body, ..), _) =
    graphql(proxy, create_contact)
  assert contact_status == 200
  let contact_json = json.to_string(contact_body)
  assert string.contains(contact_json, "\"companyContact\":null")
  assert string.contains(
    contact_json,
    "\"field\":[\"input\",\"phone\"],\"message\":\"Phone is invalid\",\"code\":\"INVALID\"",
  )
  assert string.contains(
    contact_json,
    "\"field\":[\"input\",\"locale\"],\"message\":\"Invalid locale format.\",\"code\":\"INVALID\"",
  )
  assert string.contains(
    contact_json,
    "\"field\":[\"input\",\"note\"],\"message\":\"Notes cannot contain HTML tags\",\"code\":\"CONTAINS_HTML_TAGS\"",
  )
}

pub fn b2b_company_create_validates_nested_contact_input_test() {
  let proxy = draft_proxy.new()
  let create_company =
    "mutation { companyCreate(input: { company: { name: \"HAR 614 Nested\" }, companyContact: { email: \"nested614@example.com\", phone: \"not-a-phone\", locale: \"not_a_locale\", note: \"<b>x</b>\" }, companyLocation: { name: \"HQ\" } }) { company { id } userErrors { field message code } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    graphql(proxy, create_company)
  assert status == 200
  let create_json = json.to_string(body)
  assert string.contains(create_json, "\"company\":null")
  assert string.contains(create_json, "\"code\":\"INVALID\"")
  assert string.contains(create_json, "\"code\":\"CONTAINS_HTML_TAGS\"")

  let read_query =
    "query { companies(first: 5) { nodes { id } } companiesCount { count } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read_query)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"nodes\":[]")
  assert string.contains(read_json, "\"count\":0")
}

pub fn b2b_contact_create_and_update_reject_duplicate_email_and_phone_test() {
  let proxy = draft_proxy.new()
  let create_company =
    "mutation { companyCreate(input: { company: { name: \"HAR 614 Dup\" }, companyContact: { email: \"dup614@example.com\", phone: \"(415) 555-1234\" }, companyLocation: { name: \"HQ\" } }) { company { id } userErrors { code } } }"
  let #(Response(status: company_status, body: company_body, ..), proxy) =
    graphql(proxy, create_company)
  assert company_status == 200
  assert string.contains(json.to_string(company_body), "\"userErrors\":[]")

  let duplicate_email_create =
    "mutation { companyContactCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { email: \"DUP614@example.com\", phone: \"+14155550000\" }) { companyContact { id } userErrors { field message code } } }"
  let #(Response(status: dup_email_status, body: dup_email_body, ..), proxy) =
    graphql(proxy, duplicate_email_create)
  assert dup_email_status == 200
  let dup_email_json = json.to_string(dup_email_body)
  assert string.contains(dup_email_json, "\"companyContact\":null")
  assert string.contains(
    dup_email_json,
    "\"field\":[\"input\",\"email\"],\"message\":\"Email address has already been taken.\",\"code\":\"TAKEN\"",
  )

  let duplicate_phone_create =
    "mutation { companyContactCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { email: \"dup-phone614@example.com\", phone: \"+14155551234\" }) { companyContact { id } userErrors { field message code } } }"
  let #(Response(status: dup_phone_status, body: dup_phone_body, ..), proxy) =
    graphql(proxy, duplicate_phone_create)
  assert dup_phone_status == 200
  let dup_phone_json = json.to_string(dup_phone_body)
  assert string.contains(dup_phone_json, "\"companyContact\":null")
  assert string.contains(
    dup_phone_json,
    "\"field\":[\"input\",\"phone\"],\"message\":\"Phone number has already been taken.\",\"code\":\"TAKEN\"",
  )

  let create_second =
    "mutation { companyContactCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { email: \"second614@example.com\", phone: \"(650) 555-1212\" }) { companyContact { id phone } userErrors { code } } }"
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    graphql(proxy, create_second)
  assert second_status == 200
  assert string.contains(json.to_string(second_body), "\"userErrors\":[]")

  let duplicate_email_update =
    "mutation { companyContactUpdate(companyContactId: \"gid://shopify/CompanyContact/11?shopify-draft-proxy=synthetic\", input: { email: \"dup614@example.com\" }) { companyContact { id } userErrors { field message code } } }"
  let #(
    Response(status: email_update_status, body: email_update_body, ..),
    proxy,
  ) = graphql(proxy, duplicate_email_update)
  assert email_update_status == 200
  let email_update_json = json.to_string(email_update_body)
  assert string.contains(email_update_json, "\"companyContact\":null")
  assert string.contains(
    email_update_json,
    "\"field\":[\"input\",\"email\"],\"message\":\"Email address has already been taken.\",\"code\":\"TAKEN\"",
  )

  let duplicate_phone_update =
    "mutation { companyContactUpdate(companyContactId: \"gid://shopify/CompanyContact/11?shopify-draft-proxy=synthetic\", input: { phone: \"(415) 555-1234\" }) { companyContact { id } userErrors { field message code } } }"
  let #(Response(status: phone_update_status, body: phone_update_body, ..), _) =
    graphql(proxy, duplicate_phone_update)
  assert phone_update_status == 200
  let phone_update_json = json.to_string(phone_update_body)
  assert string.contains(phone_update_json, "\"companyContact\":null")
  assert string.contains(
    phone_update_json,
    "\"field\":[\"input\",\"phone\"],\"message\":\"Phone number has already been taken.\",\"code\":\"TAKEN\"",
  )
}

pub fn b2b_assign_main_contact_rejects_wrong_company_contact_test() {
  let proxy = draft_proxy.new()
  let create_first =
    "mutation { companyCreate(input: { company: { name: \"HAR 618 First\" }, companyContact: { email: \"first618@example.com\" }, companyLocation: { name: \"HQ\" } }) { company { id mainContact { id isMainContact } } userErrors { code } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(proxy, create_first)
  assert first_status == 200
  assert string.contains(json.to_string(first_body), "\"userErrors\":[]")

  let create_second =
    "mutation { companyCreate(input: { company: { name: \"HAR 618 Second\" }, companyLocation: { name: \"HQ\" } }) { company { id } userErrors { code } } }"
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    graphql(proxy, create_second)
  assert second_status == 200
  assert string.contains(json.to_string(second_body), "\"userErrors\":[]")

  let assign_wrong_company =
    "mutation { companyAssignMainContact(companyId: \"gid://shopify/Company/9?shopify-draft-proxy=synthetic\", companyContactId: \"gid://shopify/CompanyContact/5?shopify-draft-proxy=synthetic\") { company { id mainContact { id } } userErrors { field message code } } }"
  let #(Response(status: assign_status, body: assign_body, ..), _) =
    graphql(proxy, assign_wrong_company)
  assert assign_status == 200
  let assign_json = json.to_string(assign_body)
  assert string.contains(assign_json, "\"company\":null")
  assert string.contains(assign_json, "\"field\":[\"companyContactId\"]")
  assert string.contains(assign_json, "\"code\":\"INVALID_INPUT\"")
  assert !string.contains(assign_json, "\"code\":\"RESOURCE_NOT_FOUND\"")
}

pub fn b2b_main_contact_state_uses_company_pointer_test() {
  let create =
    "mutation { companyCreate(input: { company: { name: \"HAR 618 Pointer\" }, companyContact: { email: \"pointer618@example.com\" }, companyLocation: { name: \"HQ\" } }) { company { id mainContact { id isMainContact } contacts(first: 5) { nodes { id isMainContact } } } userErrors { code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(draft_proxy.new(), create)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let state_request =
    Request(method: "GET", path: "/__meta/state", headers: dict.new(), body: "")
  let #(Response(status: state_status, body: state_body, ..), _) =
    draft_proxy.process_request(proxy, state_request)
  assert state_status == 200
  let state_json = json.to_string(state_body)
  assert string.contains(
    state_json,
    "\"mainContactId\":\"gid://shopify/CompanyContact/5?shopify-draft-proxy=synthetic\"",
  )
  assert !string.contains(state_json, "\"isMainContact\"")
}

pub fn b2b_revoke_main_contact_derives_contact_booleans_from_pointer_test() {
  let proxy = draft_proxy.new()
  let create =
    "mutation { companyCreate(input: { company: { name: \"HAR 618 Revoke\" }, companyContact: { email: \"revoke-main618@example.com\" }, companyLocation: { name: \"HQ\" } }) { company { id } userErrors { code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(proxy, create)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let create_contact =
    "mutation { companyContactCreate(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", input: { email: \"revoke-secondary618@example.com\" }) { companyContact { id isMainContact } userErrors { code } } }"
  let #(Response(status: contact_status, body: contact_body, ..), proxy) =
    graphql(proxy, create_contact)
  assert contact_status == 200
  assert string.contains(json.to_string(contact_body), "\"userErrors\":[]")

  let assign =
    "mutation { companyAssignMainContact(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", companyContactId: \"gid://shopify/CompanyContact/9?shopify-draft-proxy=synthetic\") { company { mainContact { id isMainContact } contacts(first: 5) { nodes { id isMainContact } } } userErrors { code } } }"
  let #(Response(status: assign_status, body: assign_body, ..), proxy) =
    graphql(proxy, assign)
  assert assign_status == 200
  let assign_json = json.to_string(assign_body)
  assert string.contains(assign_json, "\"userErrors\":[]")
  assert string.contains(
    assign_json,
    "\"mainContact\":{\"id\":\"gid://shopify/CompanyContact/9?shopify-draft-proxy=synthetic\",\"isMainContact\":true}",
  )
  assert string.contains(
    assign_json,
    "\"id\":\"gid://shopify/CompanyContact/5?shopify-draft-proxy=synthetic\",\"isMainContact\":false",
  )

  let revoke =
    "mutation { companyRevokeMainContact(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\") { company { mainContact { id } contacts(first: 5) { nodes { id isMainContact } } } userErrors { code } } }"
  let #(Response(status: revoke_status, body: revoke_body, ..), proxy) =
    graphql(proxy, revoke)
  assert revoke_status == 200
  let revoke_json = json.to_string(revoke_body)
  assert string.contains(revoke_json, "\"mainContact\":null")
  assert string.contains(revoke_json, "\"isMainContact\":false")
  assert !string.contains(revoke_json, "\"isMainContact\":true")

  let read =
    "query { company(id: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\") { mainContact { id } contacts(first: 5) { nodes { id isMainContact } } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(proxy, read)
  assert read_status == 200
  let read_json = json.to_string(read_body)
  assert string.contains(read_json, "\"mainContact\":null")
  assert string.contains(read_json, "\"isMainContact\":false")
  assert !string.contains(read_json, "\"isMainContact\":true")
}

pub fn b2b_deleting_main_contact_clears_company_pointer_test() {
  let create =
    "mutation { companyCreate(input: { company: { name: \"HAR 618 Delete\" }, companyContact: { email: \"delete-main618@example.com\" }, companyLocation: { name: \"HQ\" } }) { company { id mainContact { id isMainContact } } userErrors { code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    graphql(draft_proxy.new(), create)
  assert create_status == 200
  assert string.contains(json.to_string(create_body), "\"userErrors\":[]")

  let delete_contact =
    "mutation { companyContactDelete(companyContactId: \"gid://shopify/CompanyContact/5?shopify-draft-proxy=synthetic\") { deletedCompanyContactId userErrors { code } } }"
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    graphql(proxy, delete_contact)
  assert delete_status == 200
  assert string.contains(
    json.to_string(delete_body),
    "\"deletedCompanyContactId\":\"gid://shopify/CompanyContact/5?shopify-draft-proxy=synthetic\"",
  )

  let read =
    "query { company(id: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\") { mainContact { id } contacts(first: 5) { nodes { id isMainContact } } } }"
  let #(Response(status: read_status, body: read_body, ..), proxy) =
    graphql(proxy, read)
  assert read_status == 200
  assert string.contains(json.to_string(read_body), "\"mainContact\":null")

  let state_request =
    Request(method: "GET", path: "/__meta/state", headers: dict.new(), body: "")
  let #(Response(status: state_status, body: state_body, ..), _) =
    draft_proxy.process_request(proxy, state_request)
  assert state_status == 200
  let state_json = json.to_string(state_body)
  assert string.contains(state_json, "\"mainContactId\":null")
  assert !string.contains(state_json, "\"isMainContact\"")
}

pub fn b2b_assign_customer_as_contact_rejects_unknown_customer_test() {
  let create_company =
    "mutation { companyCreate(input: { company: { name: \"HAR 606 Unknown Customer\" } }) { company { id } userErrors { code } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(draft_proxy.new(), create_company)
  assert create_status == 200

  let assign_unknown =
    "mutation { companyAssignCustomerAsContact(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", customerId: \"gid://shopify/Customer/999999999\") { companyContact { id customer { id email } } userErrors { field message code } } }"
  let #(Response(status: assign_status, body: assign_body, ..), _) =
    graphql(proxy, assign_unknown)
  assert assign_status == 200
  let assign_json = json.to_string(assign_body)
  assert string.contains(assign_json, "\"companyContact\":null")
  assert string.contains(
    assign_json,
    "\"field\":[\"customerId\"],\"message\":\"Customer does not exist.\",\"code\":\"CUSTOMER_NOT_FOUND\"",
  )
}

pub fn b2b_assign_customer_as_contact_rejects_duplicate_customer_test() {
  let customer_id = "gid://shopify/Customer/6061"
  let create_company =
    "mutation { companyCreate(input: { company: { name: \"HAR 606 Duplicate Customer\" } }) { company { id } userErrors { code } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(draft_proxy.new(), create_company)
  assert create_status == 200
  let proxy =
    seed_customer(proxy, customer(customer_id, Some("har-606@example.com")))

  let assign =
    "mutation { companyAssignCustomerAsContact(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", customerId: \""
    <> customer_id
    <> "\") { companyContact { id customer { id email } } userErrors { field message code } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    graphql(proxy, assign)
  assert first_status == 200
  let first_json = json.to_string(first_body)
  assert string.contains(first_json, "\"userErrors\":[]")
  assert string.contains(first_json, "\"email\":\"har-606@example.com\"")

  let #(Response(status: second_status, body: second_body, ..), proxy) =
    graphql(proxy, assign)
  assert second_status == 200
  let second_json = json.to_string(second_body)
  assert string.contains(second_json, "\"companyContact\":null")
  assert string.contains(
    second_json,
    "\"field\":[\"companyId\"],\"message\":\"Customer is already associated with a company contact.\",\"code\":\"CUSTOMER_ALREADY_A_CONTACT\"",
  )

  let #(Response(status: read_status, body: read_body, ..), _) =
    graphql(
      proxy,
      "query { company(id: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\") { contactsCount { count } } }",
    )
  assert read_status == 200
  assert string.contains(json.to_string(read_body), "\"count\":1")
}

pub fn b2b_assign_customer_as_contact_rejects_customer_without_email_test() {
  let customer_id = "gid://shopify/Customer/6062"
  let create_company =
    "mutation { companyCreate(input: { company: { name: \"HAR 606 No Email\" } }) { company { id } userErrors { code } } }"
  let #(Response(status: create_status, ..), proxy) =
    graphql(draft_proxy.new(), create_company)
  assert create_status == 200
  let proxy = seed_customer(proxy, customer(customer_id, None))

  let assign =
    "mutation { companyAssignCustomerAsContact(companyId: \"gid://shopify/Company/1?shopify-draft-proxy=synthetic\", customerId: \""
    <> customer_id
    <> "\") { companyContact { id } userErrors { field message code } } }"
  let #(Response(status: assign_status, body: assign_body, ..), _) =
    graphql(proxy, assign)
  assert assign_status == 200
  let assign_json = json.to_string(assign_body)
  assert string.contains(assign_json, "\"companyContact\":null")
  assert string.contains(
    assign_json,
    "\"field\":[\"companyId\"],\"message\":\"Customer must have an email address.\",\"code\":\"CUSTOMER_EMAIL_MUST_EXIST\"",
  )
}

pub fn b2b_company_contact_roots_enforce_contact_cap_test() {
  let customer_id = "gid://shopify/Customer/6063"
  let proxy =
    proxy_with_company_at_contact_cap(customer(
      customer_id,
      Some("har-606-cap@example.com"),
    ))

  let assign =
    "mutation { companyAssignCustomerAsContact(companyId: \"gid://shopify/Company/606\", customerId: \""
    <> customer_id
    <> "\") { companyContact { id } userErrors { field message code } } }"
  let #(Response(status: assign_status, body: assign_body, ..), proxy) =
    graphql(proxy, assign)
  assert assign_status == 200
  let assign_json = json.to_string(assign_body)
  assert string.contains(assign_json, "\"companyContact\":null")
  assert string.contains(
    assign_json,
    "\"field\":[\"companyId\"],\"message\":\"Company contact maximum cap reached.\",\"code\":\"COMPANY_CONTACT_MAX_CAP_REACHED\"",
  )

  let create_contact =
    "mutation { companyContactCreate(companyId: \"gid://shopify/Company/606\", input: { email: \"har-606-new@example.com\" }) { companyContact { id } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), _) =
    graphql(proxy, create_contact)
  assert create_status == 200
  let create_json = json.to_string(create_body)
  assert string.contains(create_json, "\"companyContact\":null")
  assert string.contains(
    create_json,
    "\"field\":[\"companyId\"],\"message\":\"Company contact maximum cap reached.\",\"code\":\"COMPANY_CONTACT_MAX_CAP_REACHED\"",
  )
}

pub fn b2b_business_customer_user_error_code_snapshot_test() {
  let shopify_business_customer_user_error_codes = [
    "RESOURCE_NOT_FOUND",
    "TAKEN",
    "INVALID_INPUT",
    "LIMIT_REACHED",
    "NO_INPUT",
    "INTERNAL_ERROR",
    "INVALID",
    "BLANK",
    "TOO_LONG",
    "CONTAINS_HTML_TAGS",
    "INVALID_LOCALE_FORMAT",
    "DUPLICATE_EXTERNAL_ID",
    "DUPLICATE_LOCATION_EXTERNAL_ID",
    "DUPLICATE_EMAIL_ADDRESS",
    "DUPLICATE_PHONE_NUMBER",
    "CUSTOMER_NOT_FOUND",
    "CUSTOMER_ALREADY_A_CONTACT",
    "CUSTOMER_EMAIL_MUST_EXIST",
    "COMPANY_CONTACT_MAX_CAP_REACHED",
    "ROLE_ASSIGNMENTS_MAX_CAP_REACHED",
    "ONE_ROLE_ALREADY_ASSIGNED",
    "CONTACT_DOES_NOT_MATCH_COMPANY",
    "EXISTING_ORDERS",
  ]
  let proxy_codes = b2b_user_error_codes.all_values()

  assert proxy_codes == shopify_business_customer_user_error_codes
  assert list.all(proxy_codes, fn(code) {
    list.contains(shopify_business_customer_user_error_codes, code)
  })
}
