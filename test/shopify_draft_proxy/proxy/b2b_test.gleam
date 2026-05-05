import gleam/dict
import gleam/json
import gleam/list
import gleam/string
import shopify_draft_proxy/proxy/b2b
import shopify_draft_proxy/proxy/b2b_user_error_codes
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{Request, Response}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/types.{ShopLocaleRecord}

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
