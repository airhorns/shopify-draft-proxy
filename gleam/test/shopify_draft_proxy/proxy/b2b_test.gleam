import gleam/dict
import gleam/json
import gleam/string
import shopify_draft_proxy/proxy/b2b
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}

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

fn escape(value: String) -> String {
  value
  |> string.replace("\\", "\\\\")
  |> string.replace("\"", "\\\"")
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
