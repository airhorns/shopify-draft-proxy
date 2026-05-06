import gleam/dict
import gleam/json
import gleam/list
import gleam/option.{None, Some}
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, DraftProxy, Request, Response,
}
import shopify_draft_proxy/state/store
import shopify_draft_proxy/state/store/types as store_types
import shopify_draft_proxy/state/types.{
  type CapturedJsonValue, AccessScopeRecord, AppInstallationRecord, AppRecord,
  CapturedArray, CapturedInt, CapturedObject, CapturedString,
  OnlineStoreIntegrationRecord,
}

const comment_id: String = "gid://shopify/Comment/har-587"

fn proxy() -> DraftProxy {
  draft_proxy.new()
  |> draft_proxy.with_default_registry()
}

fn proxy_with_comment(status: String) -> DraftProxy {
  let proxy = proxy()
  let comment =
    types.OnlineStoreContentRecord(
      id: comment_id,
      kind: "comment",
      cursor: None,
      parent_id: Some("gid://shopify/Article/har-587"),
      created_at: Some("2026-05-05T00:00:00.000Z"),
      updated_at: Some("2026-05-05T00:00:00.000Z"),
      data: types.CapturedObject([
        #("__typename", types.CapturedString("Comment")),
        #("id", types.CapturedString(comment_id)),
        #("status", types.CapturedString(status)),
        #("isPublished", types.CapturedBool(status == "PUBLISHED")),
        #("body", types.CapturedString("HAR-587 moderation fixture")),
        #("bodyHtml", types.CapturedString("<p>HAR-587 moderation fixture</p>")),
      ]),
    )
  let seeded_store =
    store.upsert_base_online_store_content(proxy.store, [comment])
  proxy_state.DraftProxy(..proxy, store: seeded_store)
}

fn seed_staff_member(proxy: DraftProxy, staff_member_id: String) -> DraftProxy {
  let record =
    types.AdminPlatformGenericNodeRecord(
      id: staff_member_id,
      typename: "StaffMember",
      data: types.CapturedObject([
        #("id", types.CapturedString(staff_member_id)),
      ]),
    )
  let seeded_store =
    store.upsert_base_admin_platform_generic_nodes(proxy.store, [record])
  proxy_state.DraftProxy(..proxy, store: seeded_store)
}

fn graphql_request(query: String) -> Request {
  Request(
    method: "POST",
    path: "/admin/api/2025-01/graphql.json",
    headers: dict.new(),
    body: "{\"query\":\"" <> escape(query) <> "\"}",
  )
}

fn escape(value: String) -> String {
  value
  |> string.replace("\\", "\\\\")
  |> string.replace("\"", "\\\"")
}

fn meta_state_request() -> Request {
  Request(method: "GET", path: "/__meta/state", headers: dict.new(), body: "")
}

fn meta_log_request() -> Request {
  Request(method: "GET", path: "/__meta/log", headers: dict.new(), body: "")
}

fn run_graphql(proxy: DraftProxy, query: String) -> #(String, DraftProxy) {
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  assert status == 200
  #(json.to_string(body), proxy)
}

fn proxy_with_basic_article() -> DraftProxy {
  let #(blog_body, proxy) =
    run_graphql(
      proxy(),
      "mutation { blogCreate(blog: { title: \"Article Validation Blog\" }) { blog { id title } userErrors { field message code } } }",
    )
  assert blog_body
    == "{\"data\":{\"blogCreate\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"title\":\"Article Validation Blog\"},\"userErrors\":[]}}}"

  let #(article_body, proxy) =
    run_graphql(
      proxy,
      "mutation { articleCreate(article: { title: \"Article Validation\", body: \"<p>Body</p>\", blogId: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", author: { name: \"Author Name\" } }) { article { id title author { name } image } userErrors { field message code } } }",
    )
  assert article_body
    == "{\"data\":{\"articleCreate\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"title\":\"Article Validation\",\"author\":{\"name\":\"Author Name\"},\"image\":null},\"userErrors\":[]}}}"
  proxy
}

pub fn content_create_rejects_publishing_with_future_publish_date_test() {
  let page_query =
    "mutation { pageCreate(page: { title: \"Future Page\", isPublished: true, publishDate: \"2099-01-01T00:00:00Z\" }) { page { id publishedAt } userErrors { field message code } } }"
  let #(Response(status: page_status, body: page_body, ..), page_proxy) =
    draft_proxy.process_request(draft_proxy.new(), graphql_request(page_query))
  assert page_status == 200
  assert json.to_string(page_body)
    == "{\"data\":{\"pageCreate\":{\"page\":null,\"userErrors\":[{\"field\":[\"page\"],\"message\":\"Can’t set isPublished to true and also set a future publish date.\",\"code\":\"INVALID_PUBLISH_DATE\"}]}}}"
  assert store.list_effective_online_store_content(page_proxy.store, "page")
    |> list.length
    == 0
  let assert [page_log] = store.get_log(page_proxy.store)
  assert page_log.status == store_types.Failed
  assert page_log.staged_resource_ids == []

  let proxy = draft_proxy.new()
  let blog_query =
    "mutation { blogCreate(blog: { title: \"Future Articles\" }) { blog { id } userErrors { field message code } } }"
  let #(Response(status: blog_status, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(blog_query))
  assert blog_status == 200
  let article_query =
    "mutation { articleCreate(article: { title: \"Future Article\", body: \"<p>Body</p>\", blogId: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", author: { name: \"Future Author\" }, isPublished: true, publishDate: \"2099-01-01T00:00:00Z\" }) { article { id publishedAt } userErrors { field message code } } }"
  let #(Response(status: article_status, body: article_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(article_query))
  assert article_status == 200
  assert json.to_string(article_body)
    == "{\"data\":{\"articleCreate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\"],\"message\":\"Can’t set isPublished to true and also set a future publish date.\",\"code\":\"INVALID_PUBLISH_DATE\"}]}}}"
  assert store.list_effective_online_store_content(proxy.store, "article")
    |> list.length
    == 0
  let assert [blog_log, article_log] = store.get_log(proxy.store)
  assert blog_log.status == store_types.Staged
  assert article_log.status == store_types.Failed
  assert article_log.staged_resource_ids == []
}

pub fn content_update_rejects_publishing_with_future_publish_date_test() {
  let proxy = draft_proxy.new()
  let create_page =
    "mutation { pageCreate(page: { title: \"Draft Page\", isPublished: false, publishDate: \"2099-01-01T00:00:00Z\" }) { page { id publishedAt } userErrors { field message code } } }"
  let #(Response(status: create_page_status, body: create_page_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_page))
  assert create_page_status == 200
  assert json.to_string(create_page_body)
    == "{\"data\":{\"pageCreate\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"publishedAt\":\"2099-01-01T00:00:00Z\"},\"userErrors\":[]}}}"

  let update_page =
    "mutation { pageUpdate(id: \"gid://shopify/Page/1?shopify-draft-proxy=synthetic\", page: { isPublished: true, publishDate: \"2099-01-01T00:00:00Z\" }) { page { id publishedAt } userErrors { field message code } } }"
  let #(Response(status: update_page_status, body: update_page_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_page))
  assert update_page_status == 200
  assert json.to_string(update_page_body)
    == "{\"data\":{\"pageUpdate\":{\"page\":null,\"userErrors\":[{\"field\":[\"page\"],\"message\":\"Can’t set isPublished to true and also set a future publish date.\",\"code\":\"INVALID_PUBLISH_DATE\"}]}}}"
  let assert [page_record] =
    store.list_effective_online_store_content(proxy.store, "page")
  assert page_record.id == "gid://shopify/Page/1?shopify-draft-proxy=synthetic"
  let assert [page_create_log, page_update_log] = store.get_log(proxy.store)
  assert page_create_log.status == store_types.Staged
  assert page_update_log.status == store_types.Failed
  assert page_update_log.staged_resource_ids == []

  let proxy = draft_proxy.new()
  let create_blog =
    "mutation { blogCreate(blog: { title: \"Draft Articles\" }) { blog { id } userErrors { field message code } } }"
  let #(Response(status: create_blog_status, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_blog))
  assert create_blog_status == 200
  let create_article =
    "mutation { articleCreate(article: { title: \"Draft Article\", body: \"<p>Body</p>\", blogId: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", author: { name: \"Future Author\" }, isPublished: false, publishDate: \"2099-01-01T00:00:00Z\" }) { article { id publishedAt } userErrors { field message code } } }"
  let #(
    Response(status: create_article_status, body: create_article_body, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(create_article))
  assert create_article_status == 200
  assert json.to_string(create_article_body)
    == "{\"data\":{\"articleCreate\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"publishedAt\":\"2099-01-01T00:00:00Z\"},\"userErrors\":[]}}}"

  let update_article =
    "mutation { articleUpdate(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\", article: { isPublished: true, publishDate: \"2099-01-01T00:00:00Z\" }) { article { id publishedAt } userErrors { field message code } } }"
  let #(
    Response(status: update_article_status, body: update_article_body, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(update_article))
  assert update_article_status == 200
  assert json.to_string(update_article_body)
    == "{\"data\":{\"articleUpdate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\"],\"message\":\"Can’t set isPublished to true and also set a future publish date.\",\"code\":\"INVALID_PUBLISH_DATE\"}]}}}"
  let assert [article_record] =
    store.list_effective_online_store_content(proxy.store, "article")
  assert article_record.id
    == "gid://shopify/Article/3?shopify-draft-proxy=synthetic"
  let assert [_blog_log, article_create_log, article_update_log] =
    store.get_log(proxy.store)
  assert article_create_log.status == store_types.Staged
  assert article_update_log.status == store_types.Failed
  assert article_update_log.staged_resource_ids == []
}

pub fn comment_moderation_uses_core_status_enum_values_test() {
  let #(approved, proxy) =
    run_graphql(
      proxy_with_comment("UNAPPROVED"),
      "mutation { commentApprove(id: \""
        <> comment_id
        <> "\") { comment { id status } userErrors { field message code } } }",
    )
  assert approved
    == "{\"data\":{\"commentApprove\":{\"comment\":{\"id\":\"gid://shopify/Comment/har-587\",\"status\":\"PUBLISHED\"},\"userErrors\":[]}}}"

  let #(spam, proxy) =
    run_graphql(
      proxy,
      "mutation { commentSpam(id: \""
        <> comment_id
        <> "\") { comment { id status } userErrors { field message code } } }",
    )
  assert spam
    == "{\"data\":{\"commentSpam\":{\"comment\":{\"id\":\"gid://shopify/Comment/har-587\",\"status\":\"SPAM\"},\"userErrors\":[]}}}"

  let #(not_spam, proxy) =
    run_graphql(
      proxy,
      "mutation { commentNotSpam(id: \""
        <> comment_id
        <> "\") { comment { id status } userErrors { field message code } } }",
    )
  assert not_spam
    == "{\"data\":{\"commentNotSpam\":{\"comment\":{\"id\":\"gid://shopify/Comment/har-587\",\"status\":\"UNAPPROVED\"},\"userErrors\":[]}}}"

  let #(deleted, proxy) =
    run_graphql(
      proxy,
      "mutation { commentDelete(id: \""
        <> comment_id
        <> "\") { deletedCommentId userErrors { field message code } } }",
    )
  assert deleted
    == "{\"data\":{\"commentDelete\":{\"deletedCommentId\":\"gid://shopify/Comment/har-587\",\"userErrors\":[]}}}"

  let #(read_after_delete, _) =
    run_graphql(
      proxy,
      "query { comment(id: \"" <> comment_id <> "\") { id status } }",
    )
  assert read_after_delete
    == "{\"data\":{\"comment\":{\"id\":\"gid://shopify/Comment/har-587\",\"status\":\"REMOVED\"}}}"
}

pub fn removed_comment_moderation_returns_invalid_and_delete_is_idempotent_test() {
  let #(body, _) =
    run_graphql(
      proxy_with_comment("REMOVED"),
      "mutation { approve: commentApprove(id: \""
        <> comment_id
        <> "\") { comment { id status } userErrors { field message code } } spam: commentSpam(id: \""
        <> comment_id
        <> "\") { comment { id status } userErrors { field message code } } delete: commentDelete(id: \""
        <> comment_id
        <> "\") { deletedCommentId userErrors { field message code } } }",
    )
  assert body
    == "{\"data\":{\"approve\":{\"comment\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Comment has been removed\",\"code\":\"INVALID\"}]},\"spam\":{\"comment\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Comment has been removed\",\"code\":\"INVALID\"}]},\"delete\":{\"deletedCommentId\":\"gid://shopify/Comment/har-587\",\"userErrors\":[]}}}"
}

fn read_state(proxy: DraftProxy) -> String {
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_state_request())
  assert status == 200
  json.to_string(body)
}

fn read_log(proxy: DraftProxy) -> String {
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_log_request())
  assert status == 200
  json.to_string(body)
}

fn storefront_token_shape(token: String) -> Bool {
  string.starts_with(token, "shpat_")
  && string.length(token) == 22
  && {
    string.drop_start(token, 6)
    |> string.to_graphemes
    |> list.all(is_hex_character)
  }
}

fn is_hex_character(char: String) -> Bool {
  case char {
    "0"
    | "1"
    | "2"
    | "3"
    | "4"
    | "5"
    | "6"
    | "7"
    | "8"
    | "9"
    | "a"
    | "b"
    | "c"
    | "d"
    | "e"
    | "f" -> True
    _ -> False
  }
}

fn create_storefront_token_loop(
  proxy: DraftProxy,
  remaining: Int,
) -> DraftProxy {
  case remaining <= 0 {
    True -> proxy
    False -> {
      let query =
        "mutation { storefrontAccessTokenCreate(input: { title: \"Hydrogen\" }) { storefrontAccessToken { id } userErrors { code field message } } }"
      let #(_, proxy) = run_graphql(proxy, query)
      create_storefront_token_loop(proxy, remaining - 1)
    }
  }
}

fn proxy_with_current_app_scopes(scopes: List(String)) -> DraftProxy {
  let app =
    AppRecord(
      id: "gid://shopify/App/1",
      api_key: Some("local-app"),
      handle: Some("local-app"),
      title: Some("Local app"),
      developer_name: Some("test-dev"),
      embedded: Some(True),
      previously_installed: Some(False),
      requested_access_scopes: [],
    )
  let installation =
    AppInstallationRecord(
      id: "gid://shopify/AppInstallation/1",
      app_id: app.id,
      launch_url: None,
      uninstall_url: None,
      access_scopes: list.map(scopes, fn(handle) {
        AccessScopeRecord(handle: handle, description: None)
      }),
      active_subscription_ids: [],
      all_subscription_ids: [],
      one_time_purchase_ids: [],
      uninstalled_at: None,
    )
  let seeded_store =
    store.upsert_base_app_installation(store.new(), installation, app)
  DraftProxy(..proxy(), store: seeded_store)
}

fn captured_object_merge(
  data: CapturedJsonValue,
  entries: List(#(String, CapturedJsonValue)),
) -> CapturedJsonValue {
  case data {
    CapturedObject(existing) -> CapturedObject(list.append(entries, existing))
    _ -> CapturedObject(entries)
  }
}

fn proxy_with_web_pixel_extension_declaration(proxy: DraftProxy) -> DraftProxy {
  let assert [record] =
    store.list_effective_online_store_integrations(proxy.store, "webPixel")
  let record =
    OnlineStoreIntegrationRecord(
      ..record,
      data: captured_object_merge(record.data, [
        #("runtimeContexts", CapturedArray([CapturedString("LAX")])),
        #(
          "settingsDefinition",
          CapturedObject([
            #(
              "accountID",
              CapturedObject([
                #("type", CapturedString("String")),
                #("min", CapturedInt(3)),
                #("max", CapturedInt(12)),
                #("regex", CapturedString("^[a-z]+$")),
              ]),
            ),
          ]),
        ),
      ]),
    )
  let #(_, seeded_store) =
    store.upsert_staged_online_store_integration(proxy.store, record)
  DraftProxy(..proxy, store: seeded_store)
}

pub fn storefront_access_token_create_returns_unique_token_scopes_and_shop_test() {
  let query =
    "mutation { storefrontAccessTokenCreate(input: { title: \"Hydrogen\" }) { storefrontAccessToken { id title accessToken accessScopes { handle } } shop { id } userErrors { code field message } } }"
  let #(first, proxy) = run_graphql(proxy(), query)
  assert first
    == "{\"data\":{\"storefrontAccessTokenCreate\":{\"storefrontAccessToken\":{\"id\":\"gid://shopify/StorefrontAccessToken/1?shopify-draft-proxy=synthetic\",\"title\":\"Hydrogen\",\"accessToken\":\"shpat_bcc6fd83f41123b4\",\"accessScopes\":[{\"handle\":\"unauthenticated_read_product_listings\"},{\"handle\":\"unauthenticated_read_product_inventory\"}]},\"shop\":{\"id\":\"gid://shopify/Shop/92891250994\"},\"userErrors\":[]}}}"
  assert storefront_token_shape("shpat_bcc6fd83f41123b4")

  let #(second, _) = run_graphql(proxy, query)
  assert second
    == "{\"data\":{\"storefrontAccessTokenCreate\":{\"storefrontAccessToken\":{\"id\":\"gid://shopify/StorefrontAccessToken/3?shopify-draft-proxy=synthetic\",\"title\":\"Hydrogen\",\"accessToken\":\"shpat_43199f7763e24d2f\",\"accessScopes\":[{\"handle\":\"unauthenticated_read_product_listings\"},{\"handle\":\"unauthenticated_read_product_inventory\"}]},\"shop\":{\"id\":\"gid://shopify/Shop/92891250994\"},\"userErrors\":[]}}}"
  assert first != second
}

pub fn storefront_access_token_create_filters_current_app_storefront_scopes_test() {
  let seeded_proxy =
    proxy_with_current_app_scopes([
      "read_products",
      "unauthenticated_read_customers",
      "unauthenticated_read_product_inventory",
      "write_orders",
    ])
  let query =
    "mutation { storefrontAccessTokenCreate(input: { title: \"Hydrogen\" }) { storefrontAccessToken { accessScopes { handle } } userErrors { code field message } } }"
  let #(body, _) = run_graphql(seeded_proxy, query)
  assert body
    == "{\"data\":{\"storefrontAccessTokenCreate\":{\"storefrontAccessToken\":{\"accessScopes\":[{\"handle\":\"unauthenticated_read_customers\"},{\"handle\":\"unauthenticated_read_product_inventory\"}]},\"userErrors\":[]}}}"
}

pub fn storefront_access_token_create_blank_title_returns_blank_user_error_test() {
  let query =
    "mutation { storefrontAccessTokenCreate(input: { title: \"   \" }) { storefrontAccessToken { id } shop { id } userErrors { code field message } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"storefrontAccessTokenCreate\":{\"storefrontAccessToken\":null,\"shop\":{\"id\":\"gid://shopify/Shop/92891250994\"},\"userErrors\":[{\"code\":\"BLANK\",\"field\":[\"input\",\"title\"],\"message\":\"Title can't be blank\"}]}}}"
  assert store.list_effective_online_store_integrations(
      proxy.store,
      "storefrontAccessToken",
    )
    |> list.length
    == 0
  let assert [log] = store.get_log(proxy.store)
  assert log.status == store_types.Failed
  assert log.staged_resource_ids == []
}

pub fn storefront_access_token_create_reaches_limit_at_100_tokens_test() {
  let proxy = create_storefront_token_loop(proxy(), 100)
  assert store.list_effective_online_store_integrations(
      proxy.store,
      "storefrontAccessToken",
    )
    |> list.length
    == 100

  let query =
    "mutation { storefrontAccessTokenCreate(input: { title: \"One too many\" }) { storefrontAccessToken { id } userErrors { code field message } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"storefrontAccessTokenCreate\":{\"storefrontAccessToken\":null,\"userErrors\":[{\"code\":\"REACHED_LIMIT\",\"field\":[\"input\"],\"message\":\"apps.admin.graph_api_errors.storefront_access_token_create.reached_limit\"}]}}}"
  assert store.list_effective_online_store_integrations(
      proxy.store,
      "storefrontAccessToken",
    )
    |> list.length
    == 100
}

pub fn web_pixel_duplicate_create_returns_taken_error_test() {
  let query =
    "mutation { webPixelCreate(webPixel: { settings: \"{\\\"accountID\\\":\\\"abc\\\"}\" }) { webPixel { id status } userErrors { __typename code field message } } }"
  let #(first, proxy) = run_graphql(proxy(), query)
  assert first
    == "{\"data\":{\"webPixelCreate\":{\"webPixel\":{\"id\":\"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\",\"status\":\"CONNECTED\"},\"userErrors\":[]}}}"

  let #(second, _) = run_graphql(proxy, query)
  assert second
    == "{\"data\":{\"webPixelCreate\":{\"webPixel\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":\"TAKEN\",\"field\":null,\"message\":\"Web pixel is taken.\"}]}}}"
}

pub fn mobile_platform_application_create_validates_platform_input_without_staging_test() {
  let query =
    "mutation { neither: mobilePlatformApplicationCreate(input: {}) { mobilePlatformApplication { __typename } userErrors { code field message } } blankAndroid: mobilePlatformApplicationCreate(input: { android: { applicationId: \"\" } }) { mobilePlatformApplication { __typename } userErrors { code field message } } blankApple: mobilePlatformApplicationCreate(input: { apple: { appId: \"   \" } }) { mobilePlatformApplication { __typename } userErrors { code field message } } both: mobilePlatformApplicationCreate(input: { android: { applicationId: \"com.example.app\" }, apple: { appId: \"1234567890.com.example.app\" } }) { mobilePlatformApplication { __typename } userErrors { code field message } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"neither\":{\"mobilePlatformApplication\":null,\"userErrors\":[{\"code\":\"INVALID\",\"field\":[\"mobilePlatformApplication\"],\"message\":\"Specify either android or apple, not both.\"}]},\"blankAndroid\":{\"mobilePlatformApplication\":null,\"userErrors\":[{\"code\":\"BLANK\",\"field\":[\"mobilePlatformApplication\",\"android\",\"applicationId\"],\"message\":\"Application can't be blank\"}]},\"blankApple\":{\"mobilePlatformApplication\":null,\"userErrors\":[{\"code\":\"BLANK\",\"field\":[\"mobilePlatformApplication\",\"apple\",\"appId\"],\"message\":\"App can't be blank\"}]},\"both\":{\"mobilePlatformApplication\":null,\"userErrors\":[{\"code\":\"INVALID\",\"field\":[\"mobilePlatformApplication\"],\"message\":\"Specify either android or apple, not both.\"}]}}}"
  assert store.list_effective_online_store_integrations(
      proxy.store,
      "mobilePlatformApplication",
    )
    |> list.length
    == 0
  let logs = store.get_log(proxy.store)
  assert list.length(logs) == 4
  assert list.all(logs, fn(log) {
    log.status == store_types.Failed && log.staged_resource_ids == []
  })
}

pub fn mobile_platform_application_create_rejects_duplicate_platform_test() {
  let android =
    "mutation { mobilePlatformApplicationCreate(input: { android: { applicationId: \"com.example.app\", appLinksEnabled: true } }) { mobilePlatformApplication { __typename ... on AndroidApplication { applicationId appLinksEnabled } } userErrors { code field message } } }"
  let #(android_body, proxy) = run_graphql(proxy(), android)
  assert android_body
    == "{\"data\":{\"mobilePlatformApplicationCreate\":{\"mobilePlatformApplication\":{\"__typename\":\"AndroidApplication\",\"applicationId\":\"com.example.app\",\"appLinksEnabled\":true},\"userErrors\":[]}}}"

  let #(android_duplicate_body, proxy) = run_graphql(proxy, android)
  assert android_duplicate_body
    == "{\"data\":{\"mobilePlatformApplicationCreate\":{\"mobilePlatformApplication\":null,\"userErrors\":[{\"code\":\"TAKEN\",\"field\":[\"mobilePlatformApplication\",\"android\"],\"message\":\"Android has already been taken\"}]}}}"

  let apple =
    "mutation { mobilePlatformApplicationCreate(input: { apple: { appId: \"1234567890.com.example.app\" } }) { mobilePlatformApplication { __typename ... on AppleApplication { appId } } userErrors { code field message } } }"
  let #(apple_body, proxy) = run_graphql(proxy, apple)
  assert apple_body
    == "{\"data\":{\"mobilePlatformApplicationCreate\":{\"mobilePlatformApplication\":{\"__typename\":\"AppleApplication\",\"appId\":\"1234567890.com.example.app\"},\"userErrors\":[]}}}"

  let #(apple_duplicate_body, proxy) = run_graphql(proxy, apple)
  assert apple_duplicate_body
    == "{\"data\":{\"mobilePlatformApplicationCreate\":{\"mobilePlatformApplication\":null,\"userErrors\":[{\"code\":\"TAKEN\",\"field\":[\"mobilePlatformApplication\",\"apple\"],\"message\":\"Apple has already been taken\"}]}}}"
  assert store.list_effective_online_store_integrations(
      proxy.store,
      "mobilePlatformApplication",
    )
    |> list.length
    == 2
}

pub fn script_tag_create_validates_src_without_staging_test() {
  let too_long_src = "https://example.test/" <> string.repeat("a", times: 260)
  let query =
    "mutation { missing: scriptTagCreate(input: {}) { scriptTag { id } userErrors { __typename code field message } } blank: scriptTagCreate(input: { src: \"   \" }) { scriptTag { id } userErrors { __typename code field message } } invalid: scriptTagCreate(input: { src: \"not-a-url\" }) { scriptTag { id } userErrors { __typename code field message } } http: scriptTagCreate(input: { src: \"http://example.test/app.js\" }) { scriptTag { id } userErrors { __typename code field message } } tooLong: scriptTagCreate(input: { src: \""
    <> too_long_src
    <> "\" }) { scriptTag { id } userErrors { __typename code field message } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"missing\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"BLANK\",\"field\":[\"input\",\"src\"],\"message\":\"Source can't be blank\"}]},\"blank\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"BLANK\",\"field\":[\"input\",\"src\"],\"message\":\"Source can't be blank\"}]},\"invalid\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"INVALID\",\"field\":[\"input\",\"src\"],\"message\":\"Source is invalid\"}]},\"http\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"INVALID\",\"field\":[\"input\",\"src\"],\"message\":\"Source is invalid\"}]},\"tooLong\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"TOO_LONG\",\"field\":[\"input\",\"src\"],\"message\":\"Source is too long (maximum is 255 characters)\"}]}}}"
  assert store.list_effective_online_store_integrations(
      proxy.store,
      "scriptTag",
    )
    |> list.length
    == 0
}

pub fn script_tag_create_defaults_display_scope_to_online_store_test() {
  let query =
    "mutation { scriptTagCreate(input: { src: \"https://cdn.example.test/app.js\" }) { scriptTag { id src displayScope event cache } userErrors { code field message } } }"
  let #(body, proxy) = run_graphql(proxy(), query)
  assert body
    == "{\"data\":{\"scriptTagCreate\":{\"scriptTag\":{\"id\":\"gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic\",\"src\":\"https://cdn.example.test/app.js\",\"displayScope\":\"ONLINE_STORE\",\"event\":\"onload\",\"cache\":false},\"userErrors\":[]}}}"

  let state = read_state(proxy)
  assert string.contains(state, "\"displayScope\":\"online_store\"")
  assert string.contains(state, "\"event\":\"onload\"")
}

pub fn script_tag_create_rejects_invalid_display_scope_test() {
  let query =
    "mutation { scriptTagCreate(input: { src: \"https://cdn.example.test/app.js\", displayScope: \"FOO\" }) { scriptTag { id } userErrors { __typename code field message } } }"
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy(), graphql_request(query))
  assert status == 200
  assert json.to_string(body)
    == "{\"data\":{\"scriptTagCreate\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"INCLUSION\",\"field\":[\"input\",\"displayScope\"],\"message\":\"Display scope is not included in the list\"}]}}}"
  assert store.list_effective_online_store_integrations(
      proxy.store,
      "scriptTag",
    )
    |> list.length
    == 0
}

pub fn script_tag_update_validates_changed_fields_only_test() {
  let too_long_src = "https://example.test/" <> string.repeat("a", times: 260)
  let create_query =
    "mutation { scriptTagCreate(input: { src: \"https://cdn.example.test/app.js\", displayScope: ALL }) { scriptTag { id src displayScope } userErrors { code field message } } }"
  let #(create_body, proxy) = run_graphql(proxy(), create_query)
  assert create_body
    == "{\"data\":{\"scriptTagCreate\":{\"scriptTag\":{\"id\":\"gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic\",\"src\":\"https://cdn.example.test/app.js\",\"displayScope\":\"ALL\"},\"userErrors\":[]}}}"

  let invalid_update =
    "mutation { blank: scriptTagUpdate(id: \"gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic\", input: { src: \"\" }) { scriptTag { id src displayScope } userErrors { __typename code field message } } tooLong: scriptTagUpdate(id: \"gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic\", input: { src: \""
    <> too_long_src
    <> "\" }) { scriptTag { id src displayScope } userErrors { __typename code field message } } invalid: scriptTagUpdate(id: \"gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic\", input: { src: \"ftp://cdn.example.test/app.js\" }) { scriptTag { id src displayScope } userErrors { __typename code field message } } http: scriptTagUpdate(id: \"gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic\", input: { src: \"http://example.test/app.js\" }) { scriptTag { id src displayScope } userErrors { __typename code field message } } display: scriptTagUpdate(id: \"gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic\", input: { displayScope: \"STOREFRONT\" }) { scriptTag { id src displayScope } userErrors { __typename code field message } } }"
  let #(invalid_body, proxy) = run_graphql(proxy, invalid_update)
  assert invalid_body
    == "{\"data\":{\"blank\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"BLANK\",\"field\":[\"src\"],\"message\":\"Source can't be blank\"}]},\"tooLong\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"TOO_LONG\",\"field\":[\"src\"],\"message\":\"Source is too long (maximum is 255 characters)\"}]},\"invalid\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"INVALID\",\"field\":[\"src\"],\"message\":\"Source is invalid\"}]},\"http\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"INVALID\",\"field\":[\"src\"],\"message\":\"Source is invalid\"}]},\"display\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"INCLUSION\",\"field\":[\"displayScope\"],\"message\":\"Display scope is not included in the list\"}]}}}"

  let valid_update =
    "mutation { scriptTagUpdate(id: \"gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic\", input: { cache: true, event: \"onstart\" }) { scriptTag { id src displayScope event cache } userErrors { code field message } } }"
  let #(valid_body, proxy) = run_graphql(proxy, valid_update)
  assert valid_body
    == "{\"data\":{\"scriptTagUpdate\":{\"scriptTag\":{\"id\":\"gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic\",\"src\":\"https://cdn.example.test/app.js\",\"displayScope\":\"ALL\",\"event\":\"onload\",\"cache\":true},\"userErrors\":[]}}}"

  let read_query =
    "query { scriptTag(id: \"gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic\") { id src displayScope event cache } }"
  let #(read_body, _) = run_graphql(proxy, read_query)
  assert read_body
    == "{\"data\":{\"scriptTag\":{\"id\":\"gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic\",\"src\":\"https://cdn.example.test/app.js\",\"displayScope\":\"ALL\",\"event\":\"onload\",\"cache\":true}}}"
}

pub fn web_pixel_create_without_settings_needs_configuration_test() {
  let query =
    "mutation { webPixelCreate(webPixel: {}) { webPixel { id status settings } userErrors { __typename code field message } } }"
  let #(body, _) = run_graphql(proxy(), query)
  assert body
    == "{\"data\":{\"webPixelCreate\":{\"webPixel\":{\"id\":\"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\",\"status\":\"NEEDS_CONFIGURATION\",\"settings\":null},\"userErrors\":[]}}}"
}

pub fn mobile_platform_application_update_applies_apple_input_test() {
  let create_query =
    "mutation { mobilePlatformApplicationCreate(input: { apple: { appId: \"com.example.old\", universalLinksEnabled: false, sharedWebCredentialsEnabled: true, appClipsEnabled: false, appClipApplicationId: \"com.example.old.Clip\" } }) { mobilePlatformApplication { __typename ... on AppleApplication { id appId universalLinksEnabled sharedWebCredentialsEnabled appClipsEnabled appClipApplicationId } } userErrors { code field message } } }"
  let #(_, proxy) = run_graphql(proxy(), create_query)

  let update_query =
    "mutation { mobilePlatformApplicationUpdate(id: \"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\", input: { apple: { appId: \"com.example.new\", universalLinksEnabled: true, sharedWebCredentialsEnabled: false, appClipsEnabled: true, appClipApplicationId: \"com.example.new.Clip\" } }) { mobilePlatformApplication { __typename ... on AppleApplication { id appId universalLinksEnabled sharedWebCredentialsEnabled appClipsEnabled appClipApplicationId } } userErrors { code field message } } }"
  let #(update_body, proxy) = run_graphql(proxy, update_query)
  assert update_body
    == "{\"data\":{\"mobilePlatformApplicationUpdate\":{\"mobilePlatformApplication\":{\"__typename\":\"AppleApplication\",\"id\":\"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\",\"appId\":\"com.example.new\",\"universalLinksEnabled\":true,\"sharedWebCredentialsEnabled\":false,\"appClipsEnabled\":true,\"appClipApplicationId\":\"com.example.new.Clip\"},\"userErrors\":[]}}}"

  let read_query =
    "query { mobilePlatformApplication(id: \"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\") { __typename ... on AppleApplication { id appId universalLinksEnabled sharedWebCredentialsEnabled appClipsEnabled appClipApplicationId } } }"
  let #(read_body, _) = run_graphql(proxy, read_query)
  assert read_body
    == "{\"data\":{\"mobilePlatformApplication\":{\"__typename\":\"AppleApplication\",\"id\":\"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\",\"appId\":\"com.example.new\",\"universalLinksEnabled\":true,\"sharedWebCredentialsEnabled\":false,\"appClipsEnabled\":true,\"appClipApplicationId\":\"com.example.new.Clip\"}}}"
}

pub fn mobile_platform_application_update_applies_android_input_test() {
  let create_query =
    "mutation { mobilePlatformApplicationCreate(input: { android: { applicationId: \"com.example.old\", appLinksEnabled: false, sha256CertFingerprints: [\"AA:BB\"] } }) { mobilePlatformApplication { __typename ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints } } userErrors { code field message } } }"
  let #(_, proxy) = run_graphql(proxy(), create_query)

  let update_query =
    "mutation { mobilePlatformApplicationUpdate(id: \"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\", input: { android: { applicationId: \"com.example.new\", appLinksEnabled: true, sha256CertFingerprints: [\"CC:DD\", \"EE:FF\"] } }) { mobilePlatformApplication { __typename ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints } } userErrors { code field message } } }"
  let #(update_body, proxy) = run_graphql(proxy, update_query)
  assert update_body
    == "{\"data\":{\"mobilePlatformApplicationUpdate\":{\"mobilePlatformApplication\":{\"__typename\":\"AndroidApplication\",\"id\":\"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\",\"applicationId\":\"com.example.new\",\"appLinksEnabled\":true,\"sha256CertFingerprints\":[\"CC:DD\",\"EE:FF\"]},\"userErrors\":[]}}}"

  let read_query =
    "query { mobilePlatformApplication(id: \"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\") { __typename ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints } } }"
  let #(read_body, _) = run_graphql(proxy, read_query)
  assert read_body
    == "{\"data\":{\"mobilePlatformApplication\":{\"__typename\":\"AndroidApplication\",\"id\":\"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\",\"applicationId\":\"com.example.new\",\"appLinksEnabled\":true,\"sha256CertFingerprints\":[\"CC:DD\",\"EE:FF\"]}}}"
}

pub fn mobile_platform_application_update_validation_errors_test() {
  let create_android =
    "mutation { mobilePlatformApplicationCreate(input: { android: { applicationId: \"com.example.android\", appLinksEnabled: true, sha256CertFingerprints: [\"AA:BB\"] } }) { mobilePlatformApplication { __typename } userErrors { code field message } } }"
  let #(_, android_proxy) = run_graphql(proxy(), create_android)

  let mismatch =
    "mutation { mobilePlatformApplicationUpdate(id: \"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\", input: { apple: { appId: \"com.example.ios\" } }) { mobilePlatformApplication { __typename } userErrors { code field message } } }"
  let #(mismatch_body, android_proxy) = run_graphql(android_proxy, mismatch)
  assert mismatch_body
    == "{\"data\":{\"mobilePlatformApplicationUpdate\":{\"mobilePlatformApplication\":null,\"userErrors\":[{\"code\":\"INVALID\",\"field\":[\"mobilePlatformApplication\"],\"message\":\"Mobile platform application platform is invalid\"}]}}}"

  let blank_android =
    "mutation { mobilePlatformApplicationUpdate(id: \"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\", input: { android: { applicationId: \"\" } }) { mobilePlatformApplication { __typename } userErrors { code field message } } }"
  let #(blank_android_body, android_proxy) =
    run_graphql(android_proxy, blank_android)
  assert blank_android_body
    == "{\"data\":{\"mobilePlatformApplicationUpdate\":{\"mobilePlatformApplication\":null,\"userErrors\":[{\"code\":\"BLANK\",\"field\":[\"mobilePlatformApplication\",\"android\",\"applicationId\"],\"message\":\"Application ID can't be blank\"}]}}}"

  let create_apple =
    "mutation { mobilePlatformApplicationCreate(input: { apple: { appId: \"com.example.apple\", universalLinksEnabled: true, sharedWebCredentialsEnabled: true } }) { mobilePlatformApplication { __typename } userErrors { code field message } } }"
  let #(_, apple_proxy) = run_graphql(proxy(), create_apple)

  let blank_apple =
    "mutation { mobilePlatformApplicationUpdate(id: \"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\", input: { apple: { appId: \"  \" } }) { mobilePlatformApplication { __typename } userErrors { code field message } } }"
  let #(blank_apple_body, _) = run_graphql(apple_proxy, blank_apple)
  assert blank_apple_body
    == "{\"data\":{\"mobilePlatformApplicationUpdate\":{\"mobilePlatformApplication\":null,\"userErrors\":[{\"code\":\"BLANK\",\"field\":[\"mobilePlatformApplication\",\"apple\",\"appId\"],\"message\":\"App ID can't be blank\"}]}}}"

  let read_query =
    "query { mobilePlatformApplication(id: \"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\") { __typename ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints } } }"
  let #(read_body, _) = run_graphql(android_proxy, read_query)
  assert read_body
    == "{\"data\":{\"mobilePlatformApplication\":{\"__typename\":\"AndroidApplication\",\"id\":\"gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic\",\"applicationId\":\"com.example.android\",\"appLinksEnabled\":true,\"sha256CertFingerprints\":[\"AA:BB\"]}}}"
}

pub fn web_pixel_update_and_delete_errors_use_web_pixel_user_error_test() {
  let update_query =
    "mutation { webPixelUpdate(id: \"gid://shopify/WebPixel/missing\", webPixel: { settings: \"{}\" }) { webPixel { id } userErrors { __typename code field message } } }"
  let #(update_body, proxy) = run_graphql(proxy(), update_query)
  assert update_body
    == "{\"data\":{\"webPixelUpdate\":{\"webPixel\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Pixel not found\"}]}}}"

  let delete_query =
    "mutation { webPixelDelete(id: \"gid://shopify/WebPixel/missing\") { deletedWebPixelId userErrors { __typename code field message } } }"
  let #(delete_body, _) = run_graphql(proxy, delete_query)
  assert delete_body
    == "{\"data\":{\"webPixelDelete\":{\"deletedWebPixelId\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Pixel not found\"}]}}}"
}

pub fn web_pixel_update_rejects_invalid_configuration_json_test() {
  let create_query =
    "mutation { webPixelCreate(webPixel: { settings: \"{}\" }) { webPixel { id } userErrors { code field } } }"
  let #(_, proxy) = run_graphql(proxy(), create_query)

  let update_query =
    "mutation { webPixelUpdate(id: \"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\", webPixel: { settings: \"not json\" }) { webPixel { id settings status } userErrors { __typename code field } } }"
  let #(body, _) = run_graphql(proxy, update_query)
  assert body
    == "{\"data\":{\"webPixelUpdate\":{\"webPixel\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":\"INVALID_CONFIGURATION_JSON\",\"field\":[\"settings\"]}]}}}"
}

pub fn web_pixel_update_rejects_invalid_runtime_context_test() {
  let create_query =
    "mutation { webPixelCreate(webPixel: { settings: \"{}\" }) { webPixel { id } userErrors { code field } } }"
  let #(_, proxy) = run_graphql(proxy(), create_query)
  let proxy = proxy_with_web_pixel_extension_declaration(proxy)

  let update_query =
    "mutation { webPixelUpdate(id: \"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\", webPixel: { settings: \"{}\", runtimeContext: \"STRICT\" }) { webPixel { id settings status } userErrors { __typename code field } } }"
  let #(body, _) = run_graphql(proxy, update_query)
  assert body
    == "{\"data\":{\"webPixelUpdate\":{\"webPixel\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":\"INVALID_RUNTIME_CONTEXT\",\"field\":[\"webPixel\",\"runtimeContext\"]}]}}}"
}

pub fn web_pixel_update_rejects_invalid_declared_settings_test() {
  let create_query =
    "mutation { webPixelCreate(webPixel: { settings: \"{}\" }) { webPixel { id } userErrors { code field } } }"
  let #(_, proxy) = run_graphql(proxy(), create_query)
  let proxy = proxy_with_web_pixel_extension_declaration(proxy)

  let update_query =
    "mutation { webPixelUpdate(id: \"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\", webPixel: { settings: \"{\\\"accountID\\\":123}\" }) { webPixel { id settings status } userErrors { __typename code field } } }"
  let #(body, _) = run_graphql(proxy, update_query)
  assert body
    == "{\"data\":{\"webPixelUpdate\":{\"webPixel\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":\"INVALID_SETTINGS\",\"field\":[\"settings\"]}]}}}"
}

pub fn web_pixel_update_rejects_setting_range_and_regex_violations_test() {
  let create_query =
    "mutation { webPixelCreate(webPixel: { settings: \"{}\" }) { webPixel { id } userErrors { code field } } }"
  let #(_, proxy) = run_graphql(proxy(), create_query)
  let proxy = proxy_with_web_pixel_extension_declaration(proxy)

  let range_query =
    "mutation { webPixelUpdate(id: \"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\", webPixel: { settings: \"{\\\"accountID\\\":\\\"ab\\\"}\" }) { webPixel { id settings status } userErrors { __typename code field } } }"
  let #(range_body, proxy) = run_graphql(proxy, range_query)
  assert range_body
    == "{\"data\":{\"webPixelUpdate\":{\"webPixel\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":\"INVALID_SETTINGS\",\"field\":[\"settings\"]}]}}}"

  let regex_query =
    "mutation { webPixelUpdate(id: \"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\", webPixel: { settings: \"{\\\"accountID\\\":\\\"ABC\\\"}\" }) { webPixel { id settings status } userErrors { __typename code field } } }"
  let #(regex_body, _) = run_graphql(proxy, regex_query)
  assert regex_body
    == "{\"data\":{\"webPixelUpdate\":{\"webPixel\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":\"INVALID_SETTINGS\",\"field\":[\"settings\"]}]}}}"
}

pub fn web_pixel_update_parses_settings_and_derives_status_test() {
  let create_query =
    "mutation { webPixelCreate(webPixel: {}) { webPixel { id status settings } userErrors { code field } } }"
  let #(create_body, proxy) = run_graphql(proxy(), create_query)
  assert create_body
    == "{\"data\":{\"webPixelCreate\":{\"webPixel\":{\"id\":\"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\",\"status\":\"NEEDS_CONFIGURATION\",\"settings\":null},\"userErrors\":[]}}}"

  let update_query =
    "mutation { webPixelUpdate(id: \"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\", webPixel: { settings: \"{\\\"accountID\\\":\\\"abc\\\"}\" }) { webPixel { id settings status } userErrors { __typename code field } } }"
  let #(body, _) = run_graphql(proxy, update_query)
  assert body
    == "{\"data\":{\"webPixelUpdate\":{\"webPixel\":{\"id\":\"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\",\"settings\":{\"accountID\":\"abc\"},\"status\":\"CONNECTED\"},\"userErrors\":[]}}}"
}

pub fn online_store_integration_missing_ids_return_not_found_codes_test() {
  let update_query =
    "mutation { scriptTagUpdate(id: \"gid://shopify/ScriptTag/9999999999\", input: { src: \"https://example.test/a.js\" }) { scriptTag { id } userErrors { __typename code field message } } themeUpdate(id: \"gid://shopify/OnlineStoreTheme/9999999999\", input: { name: \"Missing\" }) { theme { id } userErrors { __typename code field message } } eventBridgeServerPixelUpdate(arn: \"arn:aws:events:us-east-1:123456789012:event-bus/missing\") { serverPixel { id } userErrors { code field message } } mobilePlatformApplicationUpdate(id: \"gid://shopify/MobilePlatformApplication/9999999999\", input: {}) { mobilePlatformApplication { __typename } userErrors { code field message } } }"
  let #(update_body, proxy) = run_graphql(proxy(), update_query)
  assert update_body
    == "{\"data\":{\"scriptTagUpdate\":{\"scriptTag\":null,\"userErrors\":[{\"__typename\":\"ScriptTagUserError\",\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Script tag not found\"}]},\"themeUpdate\":{\"theme\":null,\"userErrors\":[{\"__typename\":\"ThemeUserError\",\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Theme not found\"}]},\"eventBridgeServerPixelUpdate\":{\"serverPixel\":null,\"userErrors\":[{\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Server pixel not found\"}]},\"mobilePlatformApplicationUpdate\":{\"mobilePlatformApplication\":null,\"userErrors\":[{\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Mobile platform application not found\"}]}}}"

  let delete_query =
    "mutation { webPixelDelete(id: \"gid://shopify/WebPixel/9999999999\") { deletedWebPixelId userErrors { code field message } } scriptTagDelete(id: \"gid://shopify/ScriptTag/9999999999\") { deletedScriptTagId userErrors { code field message } } themeDelete(id: \"gid://shopify/OnlineStoreTheme/9999999999\") { deletedThemeId userErrors { code field message } } serverPixelDelete { deletedServerPixelId userErrors { code field message } } mobilePlatformApplicationDelete(id: \"gid://shopify/MobilePlatformApplication/9999999999\") { deletedMobilePlatformApplicationId userErrors { code field message } } }"
  let #(delete_body, _) = run_graphql(proxy, delete_query)
  assert delete_body
    == "{\"data\":{\"webPixelDelete\":{\"deletedWebPixelId\":null,\"userErrors\":[{\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Pixel not found\"}]},\"scriptTagDelete\":{\"deletedScriptTagId\":null,\"userErrors\":[{\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Script tag not found\"}]},\"themeDelete\":{\"deletedThemeId\":null,\"userErrors\":[{\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Theme not found\"}]},\"serverPixelDelete\":{\"deletedServerPixelId\":null,\"userErrors\":[{\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Server pixel not found\"}]},\"mobilePlatformApplicationDelete\":{\"deletedMobilePlatformApplicationId\":null,\"userErrors\":[{\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Mobile platform application not found\"}]}}}"
}

pub fn online_store_integration_malformed_ids_return_invalid_codes_test() {
  let query =
    "mutation { webPixelDelete(id: \"not-a-gid\") { deletedWebPixelId userErrors { __typename code field message } } themeUpdate(id: \"not-a-gid\", input: { name: \"Invalid\" }) { theme { id } userErrors { __typename code field message } } }"
  let #(body, _) = run_graphql(proxy(), query)
  assert body
    == "{\"data\":{\"webPixelDelete\":{\"deletedWebPixelId\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":\"INVALID\",\"field\":[\"id\"],\"message\":\"Invalid global id\"}]},\"themeUpdate\":{\"theme\":null,\"userErrors\":[{\"__typename\":\"ThemeUserError\",\"code\":\"INVALID\",\"field\":[\"id\"],\"message\":\"Invalid global id\"}]}}}"
}

pub fn web_pixel_state_omits_webhook_endpoint_address_test() {
  let query =
    "mutation { webPixelCreate(webPixel: { settings: \"{}\" }) { webPixel { id status webhookEndpointAddress } userErrors { field message } } }"
  let #(body, proxy) = run_graphql(proxy(), query)
  assert body
    == "{\"data\":{\"webPixelCreate\":{\"webPixel\":{\"id\":\"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\",\"status\":\"CONNECTED\",\"webhookEndpointAddress\":null},\"userErrors\":[]}}}"

  let state = read_state(proxy)
  assert string.contains(state, "onlineStoreWebPixels")
  assert string.contains(state, "webhookEndpointAddress") == False
}

pub fn server_pixel_state_keeps_webhook_endpoint_address_test() {
  let query =
    "mutation { serverPixelCreate { serverPixel { id status webhookEndpointAddress } userErrors { field message } } }"
  let #(body, proxy) = run_graphql(proxy(), query)
  assert body
    == "{\"data\":{\"serverPixelCreate\":{\"serverPixel\":{\"id\":\"gid://shopify/ServerPixel/1?shopify-draft-proxy=synthetic\",\"status\":\"CONNECTED\",\"webhookEndpointAddress\":null},\"userErrors\":[]}}}"

  let state = read_state(proxy)
  assert string.contains(state, "onlineStoreServerPixels")
  assert string.contains(state, "webhookEndpointAddress")
}

pub fn server_pixel_endpoint_updates_stage_valid_addresses_test() {
  let #(create_body, proxy) =
    run_graphql(
      proxy(),
      "mutation { serverPixelCreate { serverPixel { id webhookEndpointAddress } userErrors { code field message } } }",
    )
  assert create_body
    == "{\"data\":{\"serverPixelCreate\":{\"serverPixel\":{\"id\":\"gid://shopify/ServerPixel/1?shopify-draft-proxy=synthetic\",\"webhookEndpointAddress\":null},\"userErrors\":[]}}}"

  let arn = "arn:aws:events:us-east-1:123456789012:event-bus/local"
  let #(eventbridge_body, proxy) =
    run_graphql(
      proxy,
      "mutation { eventBridgeServerPixelUpdate(arn: \""
        <> arn
        <> "\") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } } }",
    )
  assert eventbridge_body
    == "{\"data\":{\"eventBridgeServerPixelUpdate\":{\"serverPixel\":{\"id\":\"gid://shopify/ServerPixel/1?shopify-draft-proxy=synthetic\",\"webhookEndpointAddress\":\"arn:aws:events:us-east-1:123456789012:event-bus/local\"},\"userErrors\":[]}}}"

  let #(pubsub_body, proxy) =
    run_graphql(
      proxy,
      "mutation { pubSubServerPixelUpdate(pubSubProject: \"project\", pubSubTopic: \"topic\") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } } }",
    )
  assert pubsub_body
    == "{\"data\":{\"pubSubServerPixelUpdate\":{\"serverPixel\":{\"id\":\"gid://shopify/ServerPixel/1?shopify-draft-proxy=synthetic\",\"webhookEndpointAddress\":\"project/topic\"},\"userErrors\":[]}}}"

  let state = read_state(proxy)
  assert string.contains(state, "\"webhookEndpointAddress\":\"project/topic\"")
  let log = read_log(proxy)
  assert string.contains(log, "eventBridgeServerPixelUpdate")
  assert string.contains(log, "pubSubServerPixelUpdate")
}

pub fn server_pixel_eventbridge_endpoint_update_rejects_invalid_arn_test() {
  let #(create_body, proxy) =
    run_graphql(
      proxy(),
      "mutation { serverPixelCreate { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } } }",
    )
  assert create_body
    == "{\"data\":{\"serverPixelCreate\":{\"serverPixel\":{\"id\":\"gid://shopify/ServerPixel/1?shopify-draft-proxy=synthetic\",\"webhookEndpointAddress\":null},\"userErrors\":[]}}}"

  let invalid_update =
    "mutation { malformed: eventBridgeServerPixelUpdate(arn: \"not-an-arn\") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } } blank: eventBridgeServerPixelUpdate(arn: \"\") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } } }"
  let #(Response(status: status, body: invalid_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(invalid_update))
  assert status == 200
  let serialized = json.to_string(invalid_body)
  assert string.contains(serialized, "\"message\":\"Invalid ARN 'not-an-arn'\"")
  assert string.contains(serialized, "\"message\":\"Invalid ARN ''\"")
  assert string.contains(
    serialized,
    "\"path\":[\"mutation\",\"eventBridgeServerPixelUpdate\",\"arn\"]",
  )
  assert string.contains(
    serialized,
    "\"code\":\"argumentLiteralsIncompatible\"",
  )

  let state = read_state(proxy)
  assert string.contains(state, "not-an-arn") == False
  assert string.contains(state, "\"webhookEndpointAddress\":null")
}

pub fn server_pixel_pubsub_endpoint_update_rejects_blank_fields_test() {
  let #(create_body, proxy) =
    run_graphql(
      proxy(),
      "mutation { serverPixelCreate { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } } }",
    )
  assert create_body
    == "{\"data\":{\"serverPixelCreate\":{\"serverPixel\":{\"id\":\"gid://shopify/ServerPixel/1?shopify-draft-proxy=synthetic\",\"webhookEndpointAddress\":null},\"userErrors\":[]}}}"

  let invalid_update =
    "mutation { blankProject: pubSubServerPixelUpdate(pubSubProject: \"\", pubSubTopic: \"topic\") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } } blankTopic: pubSubServerPixelUpdate(pubSubProject: \"project\", pubSubTopic: \" \") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } } bothBlank: pubSubServerPixelUpdate(pubSubProject: \"\", pubSubTopic: \"\") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } } }"
  let #(Response(status: status, body: invalid_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(invalid_update))
  assert status == 200
  let serialized = json.to_string(invalid_body)
  assert string.contains(
    serialized,
    "\"message\":\"pubSubProject can't be blank\"",
  )
  assert string.contains(
    serialized,
    "\"message\":\"pubSubTopic can't be blank\"",
  )
  assert string.contains(serialized, "\"code\":\"INVALID_FIELD_ARGUMENTS\"")
  assert string.contains(serialized, "\"path\":[\"pubSubServerPixelUpdate\"]")

  let state = read_state(proxy)
  assert string.contains(state, "/topic") == False
  assert string.contains(state, "project/") == False
  assert string.contains(state, "\"webhookEndpointAddress\":null")
}

pub fn server_pixel_endpoint_update_missing_arguments_use_schema_errors_test() {
  let #(Response(status: eventbridge_status, body: eventbridge_body, ..), _) =
    draft_proxy.process_request(
      proxy(),
      graphql_request(
        "mutation { eventBridgeServerPixelUpdate { serverPixel { id } userErrors { code field message } } }",
      ),
    )
  assert eventbridge_status == 200
  assert json.to_string(eventbridge_body)
    == "{\"errors\":[{\"message\":\"Field 'eventBridgeServerPixelUpdate' is missing required arguments: arn\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"eventBridgeServerPixelUpdate\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"eventBridgeServerPixelUpdate\",\"arguments\":\"arn\"}}]}"

  let #(Response(status: pubsub_status, body: pubsub_body, ..), _) =
    draft_proxy.process_request(
      proxy(),
      graphql_request(
        "mutation { pubSubServerPixelUpdate(pubSubProject: \"project\") { serverPixel { id } userErrors { code field message } } }",
      ),
    )
  assert pubsub_status == 200
  assert json.to_string(pubsub_body)
    == "{\"errors\":[{\"message\":\"Field 'pubSubServerPixelUpdate' is missing required arguments: pubSubTopic\",\"locations\":[{\"line\":1,\"column\":12}],\"path\":[\"mutation\",\"pubSubServerPixelUpdate\"],\"extensions\":{\"code\":\"missingRequiredArguments\",\"className\":\"Field\",\"name\":\"pubSubServerPixelUpdate\",\"arguments\":\"pubSubTopic\"}}]}"
}

pub fn server_pixel_endpoint_updates_require_existing_pixel_test() {
  let query =
    "mutation { eventBridge: eventBridgeServerPixelUpdate(arn: \"arn:aws:events:us-east-1:123456789012:event-bus/missing\") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } } pubsub: pubSubServerPixelUpdate(pubSubProject: \"project\", pubSubTopic: \"topic\") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } } }"
  let #(body, _) = run_graphql(proxy(), query)
  assert body
    == "{\"data\":{\"eventBridge\":{\"serverPixel\":null,\"userErrors\":[{\"__typename\":\"ServerPixelUserError\",\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Server pixel not found\"}]},\"pubsub\":{\"serverPixel\":null,\"userErrors\":[{\"__typename\":\"ServerPixelUserError\",\"code\":\"NOT_FOUND\",\"field\":[\"id\"],\"message\":\"Server pixel not found\"}]}}}"
}

pub fn content_create_missing_or_blank_title_returns_blank_user_error_test() {
  let page_missing_query =
    "mutation { pageCreate(page: {}) { page { id title handle } userErrors { field message code } } }"
  let #(Response(status: page_status, body: page_body, ..), page_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(page_missing_query),
    )
  assert page_status == 200
  assert json.to_string(page_body)
    == "{\"data\":{\"pageCreate\":{\"page\":null,\"userErrors\":[{\"field\":[\"page\",\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\"}]}}}"
  assert store.list_effective_online_store_content(page_proxy.store, "page")
    |> list.length
    == 0
  let assert [page_log] = store.get_log(page_proxy.store)
  assert page_log.status == store_types.Failed
  assert page_log.staged_resource_ids == []

  let blog_blank_query =
    "mutation { blogCreate(blog: { title: \"   \" }) { blog { id title handle } userErrors { field message code } } }"
  let #(Response(status: blog_status, body: blog_body, ..), blog_proxy) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(blog_blank_query),
    )
  assert blog_status == 200
  assert json.to_string(blog_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":null,\"userErrors\":[{\"field\":[\"blog\",\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\"}]}}}"
  assert store.list_effective_online_store_content(blog_proxy.store, "blog")
    |> list.length
    == 0
  let assert [blog_log] = store.get_log(blog_proxy.store)
  assert blog_log.status == store_types.Failed
  assert blog_log.staged_resource_ids == []
}

pub fn article_create_missing_title_returns_blank_user_error_before_staging_test() {
  let proxy = draft_proxy.new()
  let blog_query =
    "mutation { blogCreate(blog: { title: \"HAR 558 Blog\" }) { blog { id title } userErrors { field message code } } }"
  let #(Response(status: blog_status, body: blog_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(blog_query))
  assert blog_status == 200
  assert json.to_string(blog_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"title\":\"HAR 558 Blog\"},\"userErrors\":[]}}}"

  let article_query =
    "mutation { articleCreate(article: { blogId: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", author: { name: \"HAR 558 Author\" } }) { article { id title handle } userErrors { field message code } } }"
  let #(Response(status: article_status, body: article_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(article_query))
  assert article_status == 200
  assert json.to_string(article_body)
    == "{\"data\":{\"articleCreate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\",\"title\"],\"message\":\"Title can't be blank\",\"code\":\"BLANK\"}]}}}"
  assert store.list_effective_online_store_content(proxy.store, "blog")
    |> list.length
    == 1
  assert store.list_effective_online_store_content(proxy.store, "article")
    |> list.length
    == 0
  let assert [blog_log, article_log] = store.get_log(proxy.store)
  assert blog_log.status == store_types.Staged
  assert article_log.status == store_types.Failed
  assert article_log.staged_resource_ids == []
}

pub fn page_update_omitted_title_preserves_existing_title_test() {
  let proxy = draft_proxy.new()
  let create_query =
    "mutation { pageCreate(page: { title: \"HAR 558 Page\", body: \"<p>Old body</p>\" }) { page { id title handle body } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"pageCreate\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"title\":\"HAR 558 Page\",\"handle\":\"har-558-page\",\"body\":\"<p>Old body</p>\"},\"userErrors\":[]}}}"

  let update_query =
    "mutation { pageUpdate(id: \"gid://shopify/Page/1?shopify-draft-proxy=synthetic\", page: { body: \"<p>New body</p>\" }) { page { id title handle body } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"pageUpdate\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"title\":\"HAR 558 Page\",\"handle\":\"har-558-page\",\"body\":\"<p>New body</p>\"},\"userErrors\":[]}}}"
  let assert [record] =
    store.list_effective_online_store_content(proxy.store, "page")
  assert record.id == "gid://shopify/Page/1?shopify-draft-proxy=synthetic"
}

pub fn page_body_html_is_preserved_on_create_update_and_read_test() {
  let proxy = draft_proxy.new()
  let create_query =
    "mutation { pageCreate(page: { title: \"Verbatim Page\", body: \"<script>alert(1)</script><p onclick='alert(2)' class='safe'>Hi</p>\" }) { page { id body bodySummary } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"pageCreate\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"body\":\"<script>alert(1)</script><p onclick='alert(2)' class='safe'>Hi</p>\",\"bodySummary\":\"alert(1)Hi\"},\"userErrors\":[]}}}"

  let read_after_create =
    "query { page(id: \"gid://shopify/Page/1?shopify-draft-proxy=synthetic\") { id body bodySummary } }"
  let #(Response(status: read_create_status, body: read_create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(read_after_create))
  assert read_create_status == 200
  assert json.to_string(read_create_body)
    == "{\"data\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"body\":\"<script>alert(1)</script><p onclick='alert(2)' class='safe'>Hi</p>\",\"bodySummary\":\"alert(1)Hi\"}}}"

  let update_query =
    "mutation { pageUpdate(id: \"gid://shopify/Page/1?shopify-draft-proxy=synthetic\", page: { body: \"<div><script>outer<script>inner</script></script><iframe src='https://example.com/embed'>fallback</iframe><p onmouseover='bad'>After</p></div>\" }) { page { id body bodySummary } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"pageUpdate\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"body\":\"<div><script>outer<script>inner</script></script><iframe src='https://example.com/embed'>fallback</iframe><p onmouseover='bad'>After</p></div>\",\"bodySummary\":\"outerinnerfallbackAfter\"},\"userErrors\":[]}}}"

  let read_after_update =
    "query { page(id: \"gid://shopify/Page/1?shopify-draft-proxy=synthetic\") { id body bodySummary } }"
  let #(Response(status: read_update_status, body: read_update_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_after_update))
  assert read_update_status == 200
  assert json.to_string(read_update_body)
    == "{\"data\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"body\":\"<div><script>outer<script>inner</script></script><iframe src='https://example.com/embed'>fallback</iframe><p onmouseover='bad'>After</p></div>\",\"bodySummary\":\"outerinnerfallbackAfter\"}}}"
}

pub fn article_body_html_is_preserved_on_create_update_and_read_test() {
  let proxy = draft_proxy.new()
  let blog_query =
    "mutation { blogCreate(blog: { title: \"Verbatim Blog\" }) { blog { id title } userErrors { field message code } } }"
  let #(Response(status: blog_status, body: blog_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(blog_query))
  assert blog_status == 200
  assert json.to_string(blog_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"title\":\"Verbatim Blog\"},\"userErrors\":[]}}}"

  let create_query =
    "mutation { articleCreate(article: { title: \"Verbatim Article\", body: \"<p onclick='bad'>Hi</p><script>alert(1)</script>\", blogId: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", author: { name: \"HAR 741 Probe\" } }) { article { id body summary } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"articleCreate\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"body\":\"<p onclick='bad'>Hi</p><script>alert(1)</script>\",\"summary\":null},\"userErrors\":[]}}}"

  let update_query =
    "mutation { articleUpdate(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\", article: { body: \"<section><iframe src='x'></iframe><script>outer<script>inner</script></script><p onload='bad' data-ok='yes'>After</p></section>\" }) { article { id body } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"articleUpdate\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"body\":\"<section><iframe src='x'></iframe><script>outer<script>inner</script></script><p onload='bad' data-ok='yes'>After</p></section>\"},\"userErrors\":[]}}}"

  let read_after_update =
    "query { article(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\") { id body } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_after_update))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"body\":\"<section><iframe src='x'></iframe><script>outer<script>inner</script></script><p onload='bad' data-ok='yes'>After</p></section>\"}}}"
}

pub fn page_handles_slugify_dedupe_and_reject_taken_updates_test() {
  let proxy = draft_proxy.new()
  let create_first =
    "mutation { pageCreate(page: { title: \"About\" }) { page { id handle } userErrors { field message code } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_first))
  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"pageCreate\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"handle\":\"about\"},\"userErrors\":[]}}}"

  let create_second =
    "mutation { pageCreate(page: { title: \"About\" }) { page { id handle } userErrors { field message code } } }"
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_second))
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"pageCreate\":{\"page\":{\"id\":\"gid://shopify/Page/3?shopify-draft-proxy=synthetic\",\"handle\":\"about-1\"},\"userErrors\":[]}}}"

  let explicit_taken =
    "mutation { pageCreate(page: { title: \"Explicit\", handle: \"about\" }) { page { id handle } userErrors { field message code } } }"
  let #(Response(status: taken_status, body: taken_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(explicit_taken))
  assert taken_status == 200
  assert json.to_string(taken_body)
    == "{\"data\":{\"pageCreate\":{\"page\":null,\"userErrors\":[{\"field\":[\"page\",\"handle\"],\"message\":\"Handle has already been taken\",\"code\":\"TAKEN\"}]}}}"

  let punctuation =
    "mutation { pageCreate(page: { title: \"Hello, World!\" }) { page { id handle } userErrors { field message code } } }"
  let #(Response(status: punctuation_status, body: punctuation_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(punctuation))
  assert punctuation_status == 200
  assert json.to_string(punctuation_body)
    == "{\"data\":{\"pageCreate\":{\"page\":{\"id\":\"gid://shopify/Page/6?shopify-draft-proxy=synthetic\",\"handle\":\"hello-world\"},\"userErrors\":[]}}}"

  let update_taken =
    "mutation { pageUpdate(id: \"gid://shopify/Page/3?shopify-draft-proxy=synthetic\", page: { handle: \"about\" }) { page { id handle } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_taken))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"pageUpdate\":{\"page\":null,\"userErrors\":[{\"field\":[\"page\",\"handle\"],\"message\":\"Handle has already been taken\",\"code\":\"TAKEN\"}]}}}"

  assert store.list_effective_online_store_content(proxy.store, "page")
    |> list.length
    == 3
}

pub fn blog_handles_slugify_dedupe_and_reject_taken_updates_test() {
  let proxy = draft_proxy.new()
  let create_first =
    "mutation { blogCreate(blog: { title: \"News & Notes\" }) { blog { id handle } userErrors { field message code } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_first))
  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"handle\":\"news-notes\"},\"userErrors\":[]}}}"

  let create_second =
    "mutation { blogCreate(blog: { title: \"News & Notes\" }) { blog { id handle } userErrors { field message code } } }"
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_second))
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":{\"id\":\"gid://shopify/Blog/3?shopify-draft-proxy=synthetic\",\"handle\":\"news-notes-1\"},\"userErrors\":[]}}}"

  let create_taken =
    "mutation { blogCreate(blog: { title: \"Explicit\", handle: \"news-notes\" }) { blog { id handle } userErrors { field message code } } }"
  let #(
    Response(status: create_taken_status, body: create_taken_body, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(create_taken))
  assert create_taken_status == 200
  assert json.to_string(create_taken_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":null,\"userErrors\":[{\"field\":[\"blog\",\"handle\"],\"message\":\"Handle has already been taken\",\"code\":\"TAKEN\"}]}}}"

  let update_taken =
    "mutation { blogUpdate(id: \"gid://shopify/Blog/3?shopify-draft-proxy=synthetic\", blog: { handle: \"news-notes\" }) { blog { id handle } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_taken))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"blogUpdate\":{\"blog\":null,\"userErrors\":[{\"field\":[\"blog\",\"handle\"],\"message\":\"Handle has already been taken\",\"code\":\"TAKEN\"}]}}}"

  assert store.list_effective_online_store_content(proxy.store, "blog")
    |> list.length
    == 2
}

pub fn blog_update_commentable_maps_to_comment_policy_test() {
  let proxy = draft_proxy.new()
  let create_query =
    "mutation { blogCreate(blog: { title: \"Commentable Blog\", commentPolicy: CLOSED }) { blog { id title commentPolicy } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"title\":\"Commentable Blog\",\"commentPolicy\":\"CLOSED\"},\"userErrors\":[]}}}"

  let update_query =
    "mutation { blogUpdate(id: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", blog: { commentable: MODERATE }) { blog { id commentPolicy } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"blogUpdate\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"commentPolicy\":\"MODERATED\"},\"userErrors\":[]}}}"

  let read_query =
    "query { blog(id: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\") { id commentPolicy } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_query))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"commentPolicy\":\"MODERATED\"}}}"
}

pub fn blog_update_invalid_commentable_reports_commentable_field_test() {
  let proxy = draft_proxy.new()
  let create_query =
    "mutation { blogCreate(blog: { title: \"Invalid Commentable Blog\", commentPolicy: CLOSED }) { blog { id commentPolicy } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"commentPolicy\":\"CLOSED\"},\"userErrors\":[]}}}"

  let update_query =
    "mutation { blogUpdate(id: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", blog: { commentable: INVALID_VALUE }) { blog { id commentPolicy } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"blogUpdate\":{\"blog\":null,\"userErrors\":[{\"field\":[\"blog\",\"commentable\"],\"message\":\"Commentable is not included in the list\",\"code\":\"INCLUSION\"}]}}}"

  let read_query =
    "query { blog(id: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\") { id commentPolicy } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_query))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"commentPolicy\":\"CLOSED\"}}}"
}

pub fn article_handles_dedupe_per_blog_and_reject_taken_updates_test() {
  let proxy = draft_proxy.new()
  let create_blog =
    "mutation { blogCreate(blog: { title: \"Articles\" }) { blog { id handle } userErrors { field message code } } }"
  let #(Response(status: blog_status, body: blog_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_blog))
  assert blog_status == 200
  assert json.to_string(blog_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"handle\":\"articles\"},\"userErrors\":[]}}}"

  let create_first =
    "mutation { articleCreate(article: { title: \"About\", body: \"<p>Body</p>\", blogId: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", author: { name: \"HAR 551 Author\" } }) { article { id handle } userErrors { field message code } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_first))
  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"articleCreate\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"handle\":\"about\"},\"userErrors\":[]}}}"

  let create_second =
    "mutation { articleCreate(article: { title: \"About\", body: \"<p>Body</p>\", blogId: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", author: { name: \"HAR 551 Author\" } }) { article { id handle } userErrors { field message code } } }"
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_second))
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"articleCreate\":{\"article\":{\"id\":\"gid://shopify/Article/5?shopify-draft-proxy=synthetic\",\"handle\":\"about-1\"},\"userErrors\":[]}}}"

  let create_taken =
    "mutation { articleCreate(article: { title: \"Explicit\", handle: \"about\", body: \"<p>Body</p>\", blogId: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", author: { name: \"HAR 551 Author\" } }) { article { id handle } userErrors { field message code } } }"
  let #(
    Response(status: create_taken_status, body: create_taken_body, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(create_taken))
  assert create_taken_status == 200
  assert json.to_string(create_taken_body)
    == "{\"data\":{\"articleCreate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\",\"handle\"],\"message\":\"Handle has already been taken\",\"code\":\"TAKEN\"}]}}}"

  let update_taken =
    "mutation { articleUpdate(id: \"gid://shopify/Article/5?shopify-draft-proxy=synthetic\", article: { handle: \"about\" }) { article { id handle } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_taken))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"articleUpdate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\",\"handle\"],\"message\":\"Handle has already been taken\",\"code\":\"TAKEN\"}]}}}"

  let create_other_blog =
    "mutation { blogCreate(blog: { title: \"Other Articles\" }) { blog { id handle } userErrors { field message code } } }"
  let #(Response(status: other_blog_status, body: other_blog_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_other_blog))
  assert other_blog_status == 200
  assert json.to_string(other_blog_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":{\"id\":\"gid://shopify/Blog/9?shopify-draft-proxy=synthetic\",\"handle\":\"other-articles\"},\"userErrors\":[]}}}"
  let same_handle_other_blog =
    "mutation { articleCreate(article: { title: \"Other\", handle: \"about\", body: \"<p>Body</p>\", blogId: \"gid://shopify/Blog/9?shopify-draft-proxy=synthetic\", author: { name: \"HAR 551 Author\" } }) { article { id handle } userErrors { field message code } } }"
  let #(Response(status: scoped_status, body: scoped_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(same_handle_other_blog))
  assert scoped_status == 200
  assert json.to_string(scoped_body)
    == "{\"data\":{\"articleCreate\":{\"article\":{\"id\":\"gid://shopify/Article/11?shopify-draft-proxy=synthetic\",\"handle\":\"about\"},\"userErrors\":[]}}}"

  assert store.list_effective_online_store_content(proxy.store, "article")
    |> list.length
    == 3
}

pub fn article_create_validates_blog_and_author_before_staging_test() {
  let missing_blog_query =
    "mutation { articleCreate(article: { title: \"Missing Blog\", body: \"<p>Body</p>\", author: { name: \"HAR 557 Author\" } }) { article { id } userErrors { field message code } } }"
  let #(
    Response(status: missing_blog_status, body: missing_blog_body, ..),
    missing_blog_proxy,
  ) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(missing_blog_query),
    )
  assert missing_blog_status == 200
  assert json.to_string(missing_blog_body)
    == "{\"data\":{\"articleCreate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\"],\"message\":\"Must reference or create a blog when creating an article.\",\"code\":\"BLOG_REFERENCE_REQUIRED\"}]}}}"
  assert store.list_effective_online_store_content(
      missing_blog_proxy.store,
      "article",
    )
    |> list.length
    == 0
  assert store.list_effective_online_store_content(
      missing_blog_proxy.store,
      "blog",
    )
    |> list.length
    == 0
  let assert [missing_blog_log] = store.get_log(missing_blog_proxy.store)
  assert missing_blog_log.status == store_types.Failed
  assert missing_blog_log.staged_resource_ids == []

  let ambiguous_blog_query =
    "mutation { articleCreate(article: { title: \"Ambiguous Blog\", body: \"<p>Body</p>\", blogId: \"gid://shopify/Blog/1\", author: { name: \"HAR 557 Author\" } }, blog: { title: \"Inline Blog\" }) { article { id } userErrors { field message code } } }"
  let #(
    Response(status: ambiguous_blog_status, body: ambiguous_blog_body, ..),
    ambiguous_blog_proxy,
  ) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(ambiguous_blog_query),
    )
  assert ambiguous_blog_status == 200
  assert json.to_string(ambiguous_blog_body)
    == "{\"data\":{\"articleCreate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\"],\"message\":\"Can't create a blog from input if a blog ID is supplied.\",\"code\":\"AMBIGUOUS_BLOG\"}]}}}"
  assert store.list_effective_online_store_content(
      ambiguous_blog_proxy.store,
      "article",
    )
    |> list.length
    == 0
  assert store.list_effective_online_store_content(
      ambiguous_blog_proxy.store,
      "blog",
    )
    |> list.length
    == 0
  let assert [ambiguous_blog_log] = store.get_log(ambiguous_blog_proxy.store)
  assert ambiguous_blog_log.status == store_types.Failed
  assert ambiguous_blog_log.staged_resource_ids == []

  let missing_author_query =
    "mutation { articleCreate(article: { title: \"Missing Author\", body: \"<p>Body</p>\", blogId: \"gid://shopify/Blog/1\", author: {} }) { article { id } userErrors { field message code } } }"
  let #(
    Response(status: missing_author_status, body: missing_author_body, ..),
    missing_author_proxy,
  ) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(missing_author_query),
    )
  assert missing_author_status == 200
  assert json.to_string(missing_author_body)
    == "{\"data\":{\"articleCreate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\"],\"message\":\"Can't create an article if both author name and user ID are blank.\",\"code\":\"AUTHOR_FIELD_REQUIRED\"}]}}}"
  assert store.list_effective_online_store_content(
      missing_author_proxy.store,
      "article",
    )
    |> list.length
    == 0
  let assert [missing_author_log] = store.get_log(missing_author_proxy.store)
  assert missing_author_log.status == store_types.Failed
  assert missing_author_log.staged_resource_ids == []

  let ambiguous_author_query =
    "mutation { articleCreate(article: { title: \"Ambiguous Author\", body: \"<p>Body</p>\", blogId: \"gid://shopify/Blog/1\", author: { name: \"HAR 557 Author\", userId: \"gid://shopify/StaffMember/1\" } }) { article { id } userErrors { field message code } } }"
  let #(
    Response(status: ambiguous_author_status, body: ambiguous_author_body, ..),
    ambiguous_author_proxy,
  ) =
    draft_proxy.process_request(
      draft_proxy.new(),
      graphql_request(ambiguous_author_query),
    )
  assert ambiguous_author_status == 200
  assert json.to_string(ambiguous_author_body)
    == "{\"data\":{\"articleCreate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\"],\"message\":\"Can't create an article author if both author name and user ID are supplied.\",\"code\":\"AMBIGUOUS_AUTHOR\"}]}}}"
  assert store.list_effective_online_store_content(
      ambiguous_author_proxy.store,
      "article",
    )
    |> list.length
    == 0
  let assert [ambiguous_author_log] =
    store.get_log(ambiguous_author_proxy.store)
  assert ambiguous_author_log.status == store_types.Failed
  assert ambiguous_author_log.staged_resource_ids == []
}

pub fn article_create_with_blog_id_and_author_name_still_stages_test() {
  let proxy = draft_proxy.new()
  let blog_query =
    "mutation { blogCreate(blog: { title: \"HAR 557 Blog\" }) { blog { id title } userErrors { field message code } } }"
  let #(Response(status: blog_status, body: blog_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(blog_query))
  assert blog_status == 200
  assert json.to_string(blog_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"title\":\"HAR 557 Blog\"},\"userErrors\":[]}}}"

  let article_query =
    "mutation { articleCreate(article: { title: \"HAR 557 Article\", body: \"<p>Body</p>\", blogId: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", author: { name: \"HAR 557 Author\" } }) { article { id title author { name } blog { id title } } userErrors { field message code } } }"
  let #(Response(status: article_status, body: article_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(article_query))
  assert article_status == 200
  assert json.to_string(article_body)
    == "{\"data\":{\"articleCreate\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"title\":\"HAR 557 Article\",\"author\":{\"name\":\"HAR 557 Author\"},\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"title\":\"HAR 557 Blog\"}},\"userErrors\":[]}}}"
  assert store.list_effective_online_store_content(proxy.store, "article")
    |> list.length
    == 1
  assert store.list_effective_online_store_content(proxy.store, "blog")
    |> list.length
    == 1
  let assert [blog_log, article_log] = store.get_log(proxy.store)
  assert blog_log.status == store_types.Staged
  assert article_log.status == store_types.Staged
  assert article_log.staged_resource_ids
    == ["gid://shopify/Article/3?shopify-draft-proxy=synthetic"]
}

pub fn article_update_validates_author_blog_and_image_before_staging_test() {
  let proxy = proxy_with_basic_article()
  let ambiguous_author =
    "mutation { articleUpdate(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\", article: { author: { name: \"Alice\" }, authorV2: { userId: \"gid://shopify/StaffMember/1\" } }) { article { id title } userErrors { field message code } } }"
  let #(ambiguous_author_body, proxy) = run_graphql(proxy, ambiguous_author)
  assert ambiguous_author_body
    == "{\"data\":{\"articleUpdate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\",\"author\"],\"message\":\"You must specify either an author name or an author user, not both.\",\"code\":\"AMBIGUOUS_AUTHOR\"},{\"field\":[\"article\",\"authorV2\"],\"message\":\"You must specify either an author name or an author user, not both.\",\"code\":\"AMBIGUOUS_AUTHOR\"}]}}}"

  let unknown_author =
    "mutation { articleUpdate(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\", article: { authorV2: { userId: \"gid://shopify/StaffMember/9999\" } }) { article { id title } userErrors { field message code } } }"
  let #(unknown_author_body, proxy) = run_graphql(proxy, unknown_author)
  assert unknown_author_body
    == "{\"data\":{\"articleUpdate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\",\"authorV2\",\"userId\"],\"message\":\"Author must exist\",\"code\":\"NOT_FOUND\"}]}}}"

  let ambiguous_blog =
    "mutation { articleUpdate(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\", article: { blogId: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", blog: { title: \"Inline Blog\" } }) { article { id title } userErrors { field message code } } }"
  let #(ambiguous_blog_body, proxy) = run_graphql(proxy, ambiguous_blog)
  assert ambiguous_blog_body
    == "{\"data\":{\"articleUpdate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\",\"blogId\"],\"message\":\"You must specify either a blogId or a blog, not both.\",\"code\":\"AMBIGUOUS_BLOG\"},{\"field\":[\"article\",\"blog\"],\"message\":\"You must specify either a blogId or a blog, not both.\",\"code\":\"AMBIGUOUS_BLOG\"}]}}}"

  let missing_image_url =
    "mutation { articleUpdate(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\", article: { image: { altText: \"Alt only\" } }) { article { id title image } userErrors { field message code } } }"
  let #(missing_image_url_body, proxy) = run_graphql(proxy, missing_image_url)
  assert missing_image_url_body
    == "{\"data\":{\"articleUpdate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\",\"image\"],\"message\":\"Cannot update image alt text without an existing image or providing a new image URL\",\"code\":\"INVALID\"}]}}}"

  let #(article_body, _) =
    run_graphql(
      proxy,
      "query { article(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\") { id title author { name } image } }",
    )
  assert article_body
    == "{\"data\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"title\":\"Article Validation\",\"author\":{\"name\":\"Author Name\"},\"image\":null}}}"
}

pub fn article_update_validates_public_author_shape_before_staging_test() {
  let proxy = proxy_with_basic_article()
  let ambiguous_author =
    "mutation { articleUpdate(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\", article: { author: { name: \"Alice\", userId: \"gid://shopify/StaffMember/1\" } }) { article { id title } userErrors { field message code } } }"
  let #(ambiguous_author_body, proxy) = run_graphql(proxy, ambiguous_author)
  assert ambiguous_author_body
    == "{\"data\":{\"articleUpdate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\"],\"message\":\"Can't update an article author if both author name and user ID are supplied.\",\"code\":\"AMBIGUOUS_AUTHOR\"}]}}}"

  let unknown_author =
    "mutation { articleUpdate(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\", article: { author: { userId: \"gid://shopify/StaffMember/9999\" } }) { article { id title } userErrors { field message code } } }"
  let #(unknown_author_body, proxy) = run_graphql(proxy, unknown_author)
  assert unknown_author_body
    == "{\"data\":{\"articleUpdate\":{\"article\":null,\"userErrors\":[{\"field\":[\"article\"],\"message\":\"User must exist if a user ID is supplied.\",\"code\":\"AUTHOR_MUST_EXIST\"}]}}}"

  let #(article_body, _) =
    run_graphql(
      proxy,
      "query { article(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\") { id title author { name } image } }",
    )
  assert article_body
    == "{\"data\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"title\":\"Article Validation\",\"author\":{\"name\":\"Author Name\"},\"image\":null}}}"
}

pub fn article_update_accepts_existing_staff_author_user_test() {
  let staff_id = "gid://shopify/StaffMember/1"
  let proxy = proxy_with_basic_article() |> seed_staff_member(staff_id)
  let update =
    "mutation { articleUpdate(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\", article: { title: \"Article Validation Updated\", authorV2: { userId: \""
    <> staff_id
    <> "\" } }) { article { id title } userErrors { field message code } } }"
  let #(update_body, proxy) = run_graphql(proxy, update)
  assert update_body
    == "{\"data\":{\"articleUpdate\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"title\":\"Article Validation Updated\"},\"userErrors\":[]}}}"
  assert store.list_effective_online_store_content(proxy.store, "article")
    |> list.length
    == 1
}

pub fn theme_publish_demotes_previous_main_and_filters_main_reads_test() {
  let proxy = draft_proxy.new()
  let first_id =
    "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic"
  let second_id =
    "gid://shopify/OnlineStoreTheme/3?shopify-draft-proxy=synthetic"
  let first_create =
    "mutation { themeCreate(source: \"https://example.com/current.zip\", name: \"Current main\", role: MAIN) { theme { id role name } userErrors { field message } } }"
  let second_create =
    "mutation { themeCreate(source: \"https://example.com/next.zip\", name: \"Next main\", role: UNPUBLISHED) { theme { id role name } userErrors { field message } } }"
  let #(Response(status: first_status, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(first_create))
  assert first_status == 200
  let #(Response(status: second_status, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(second_create))
  assert second_status == 200

  let publish =
    "mutation { themePublish(id: \""
    <> second_id
    <> "\") { theme { id role } userErrors { field message } } }"
  let #(Response(status: publish_status, body: publish_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(publish))
  assert publish_status == 200
  assert json.to_string(publish_body)
    == "{\"data\":{\"themePublish\":{\"theme\":{\"id\":\"gid://shopify/OnlineStoreTheme/3?shopify-draft-proxy=synthetic\",\"role\":\"MAIN\"},\"userErrors\":[]}}}"

  let read =
    "query { previous: theme(id: \""
    <> first_id
    <> "\") { id role name } mains: themes(first: 10, roles: [MAIN]) { nodes { id role name } } }"
  let #(Response(status: read_status, body: read_body, ..), _proxy) =
    draft_proxy.process_request(proxy, graphql_request(read))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"previous\":{\"id\":\"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\",\"role\":\"UNPUBLISHED\",\"name\":\"Current main\"},\"mains\":{\"nodes\":[{\"id\":\"gid://shopify/OnlineStoreTheme/3?shopify-draft-proxy=synthetic\",\"role\":\"MAIN\",\"name\":\"Next main\"}]}}}"
}

pub fn theme_publish_rejects_demo_locked_or_archived_theme_test() {
  let proxy = draft_proxy.new()
  let theme_id =
    "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic"
  let create =
    "mutation { themeCreate(source: \"https://example.com/demo.zip\", name: \"Demo theme\", role: DEMO) { theme { id role } userErrors { field message } } }"
  let #(_, proxy) = draft_proxy.process_request(proxy, graphql_request(create))
  let publish =
    "mutation { themePublish(id: \""
    <> theme_id
    <> "\") { theme { id role } userErrors { field message } } }"
  let #(Response(status: publish_status, body: publish_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(publish))
  assert publish_status == 200
  assert json.to_string(publish_body)
    == "{\"data\":{\"themePublish\":{\"theme\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Theme cannot be published from role DEMO\"}]}}}"

  let read =
    "query { theme(id: \""
    <> theme_id
    <> "\") { id role } themes(first: 5, roles: [MAIN]) { nodes { id role } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"theme\":{\"id\":\"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\",\"role\":\"DEMO\"},\"themes\":{\"nodes\":[]}}}"
}

pub fn theme_update_ignores_role_input_test() {
  let proxy = draft_proxy.new()
  let theme_id =
    "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic"
  let create =
    "mutation { themeCreate(source: \"https://example.com/theme.zip\", name: \"Role fixture\", role: UNPUBLISHED) { theme { id role name } userErrors { field message code } } }"
  let #(_, proxy) = draft_proxy.process_request(proxy, graphql_request(create))

  let update =
    "mutation { themeUpdate(id: \""
    <> theme_id
    <> "\", input: { role: MAIN, processing: true }) { theme { id role name } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"themeUpdate\":{\"theme\":{\"id\":\"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\",\"role\":\"UNPUBLISHED\",\"name\":\"Role fixture\"},\"userErrors\":[]}}}"

  let read = "query { theme(id: \"" <> theme_id <> "\") { id role name } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"theme\":{\"id\":\"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\",\"role\":\"UNPUBLISHED\",\"name\":\"Role fixture\"}}}"
}

pub fn theme_update_rejects_locked_theme_test() {
  let proxy = draft_proxy.new()
  let theme_id =
    "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic"
  let create =
    "mutation { themeCreate(source: \"https://example.com/theme.zip\", name: \"Locked fixture\", role: LOCKED) { theme { id role name } userErrors { field message code } } }"
  let #(_, proxy) = draft_proxy.process_request(proxy, graphql_request(create))

  let update =
    "mutation { themeUpdate(id: \""
    <> theme_id
    <> "\", input: { name: \"Renamed\" }) { theme { id role name } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"themeUpdate\":{\"theme\":null,\"userErrors\":[{\"field\":[\"id\"],\"message\":\"Locked themes cannot be modified.\",\"code\":\"CANNOT_UPDATE_LOCKED_THEME\"}]}}}"

  let read = "query { theme(id: \"" <> theme_id <> "\") { id role name } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"theme\":{\"id\":\"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\",\"role\":\"LOCKED\",\"name\":\"Locked fixture\"}}}"
}

pub fn theme_update_rejects_blank_name_test() {
  let proxy = draft_proxy.new()
  let theme_id =
    "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic"
  let create =
    "mutation { themeCreate(source: \"https://example.com/theme.zip\", name: \"Blank fixture\", role: UNPUBLISHED) { theme { id role name } userErrors { field message code } } }"
  let #(_, proxy) = draft_proxy.process_request(proxy, graphql_request(create))

  let update =
    "mutation { themeUpdate(id: \""
    <> theme_id
    <> "\", input: { name: \"   \" }) { theme { id role name } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"themeUpdate\":{\"theme\":null,\"userErrors\":[{\"field\":[\"input\",\"name\"],\"message\":\"Name can't be blank\",\"code\":\"INVALID\"}]}}}"

  let read = "query { theme(id: \"" <> theme_id <> "\") { id role name } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"theme\":{\"id\":\"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\",\"role\":\"UNPUBLISHED\",\"name\":\"Blank fixture\"}}}"
}

pub fn theme_update_valid_name_rename_still_stages_test() {
  let proxy = draft_proxy.new()
  let theme_id =
    "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic"
  let create =
    "mutation { themeCreate(source: \"https://example.com/theme.zip\", name: \"Original fixture\", role: UNPUBLISHED) { theme { id role name } userErrors { field message code } } }"
  let #(_, proxy) = draft_proxy.process_request(proxy, graphql_request(create))

  let update =
    "mutation { themeUpdate(id: \""
    <> theme_id
    <> "\", input: { name: \"Renamed fixture\" }) { theme { id role name } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"themeUpdate\":{\"theme\":{\"id\":\"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\",\"role\":\"UNPUBLISHED\",\"name\":\"Renamed fixture\"},\"userErrors\":[]}}}"

  let read = "query { theme(id: \"" <> theme_id <> "\") { id role name } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"theme\":{\"id\":\"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\",\"role\":\"UNPUBLISHED\",\"name\":\"Renamed fixture\"}}}"
}

pub fn theme_files_upsert_uses_body_checksum_size_and_validates_filename_test() {
  let proxy = draft_proxy.new()
  let create =
    "mutation { themeCreate(source: \"https://example.com/theme.zip\", name: \"HAR 585 Theme\") { theme { id } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"themeCreate\":{\"theme\":{\"id\":\"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\"},\"userErrors\":[]}}}"

  let first =
    "mutation { themeFilesUpsert(themeId: \"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\", files: [{ filename: \"templates/index.json\", body: { type: TEXT, value: \"hello\" } }]) { upsertedThemeFiles { filename checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } } }"
  let #(Response(status: first_status, body: first_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(first))
  assert first_status == 200
  assert json.to_string(first_body)
    == "{\"data\":{\"themeFilesUpsert\":{\"upsertedThemeFiles\":[{\"filename\":\"templates/index.json\",\"checksumMd5\":\"5d41402abc4b2a76b9719d911017c592\",\"size\":5,\"body\":{\"content\":\"hello\"}}],\"userErrors\":[]}}}"

  let second =
    "mutation { themeFilesUpsert(themeId: \"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\", files: [{ filename: \"templates/index.json\", body: { type: TEXT, value: \"hello world\" } }]) { upsertedThemeFiles { filename checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } } }"
  let #(Response(status: second_status, body: second_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(second))
  assert second_status == 200
  assert json.to_string(second_body)
    == "{\"data\":{\"themeFilesUpsert\":{\"upsertedThemeFiles\":[{\"filename\":\"templates/index.json\",\"checksumMd5\":\"5eb63bbbe01eeed093cb22bb8f5acdc3\",\"size\":11,\"body\":{\"content\":\"hello world\"}}],\"userErrors\":[]}}}"

  let invalid =
    "mutation { themeFilesUpsert(themeId: \"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\", files: [{ filename: \"evil/path.liquid\", body: { type: TEXT, value: \"ignored\" } }]) { upsertedThemeFiles { filename } userErrors { field message code } } }"
  let #(Response(status: invalid_status, body: invalid_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(invalid))
  assert invalid_status == 200
  assert json.to_string(invalid_body)
    == "{\"data\":{\"themeFilesUpsert\":{\"upsertedThemeFiles\":[],\"userErrors\":[{\"field\":[\"files\",\"0\",\"filename\"],\"message\":\"Filename is invalid\",\"code\":\"INVALID\"}]}}}"

  let read =
    "query { theme(id: \"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\") { files(first: 10) { nodes { filename checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"theme\":{\"files\":{\"nodes\":[{\"filename\":\"templates/index.json\",\"checksumMd5\":\"5eb63bbbe01eeed093cb22bb8f5acdc3\",\"size\":11,\"body\":{\"content\":\"hello world\"}}]}}}}"
}

pub fn theme_files_copy_and_delete_validate_local_file_lifecycle_test() {
  let proxy = draft_proxy.new()
  let create =
    "mutation { themeCreate(source: \"https://example.com/theme.zip\", name: \"HAR 585 Theme\") { theme { id } userErrors { field message code } } }"
  let #(_, proxy) = draft_proxy.process_request(proxy, graphql_request(create))
  let upsert =
    "mutation { themeFilesUpsert(themeId: \"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\", files: [{ filename: \"assets/app.js\", body: { type: TEXT, value: \"console.log(1)\" } }]) { upsertedThemeFiles { filename } userErrors { field message code } } }"
  let #(_, proxy) = draft_proxy.process_request(proxy, graphql_request(upsert))

  let missing_copy =
    "mutation { themeFilesCopy(themeId: \"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\", files: [{ srcFilename: \"assets/missing.js\", dstFilename: \"assets/copy.js\" }]) { copiedThemeFiles { filename } userErrors { field message code } } }"
  let #(
    Response(status: missing_copy_status, body: missing_copy_body, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(missing_copy))
  assert missing_copy_status == 200
  assert json.to_string(missing_copy_body)
    == "{\"data\":{\"themeFilesCopy\":{\"copiedThemeFiles\":[],\"userErrors\":[{\"field\":[\"files\",\"0\",\"srcFilename\"],\"message\":\"File not found\",\"code\":\"NOT_FOUND\"}]}}}"

  let copy =
    "mutation { themeFilesCopy(themeId: \"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\", files: [{ srcFilename: \"assets/app.js\", dstFilename: \"assets/copy.js\" }]) { copiedThemeFiles { filename checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } } }"
  let #(Response(status: copy_status, body: copy_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(copy))
  assert copy_status == 200
  assert json.to_string(copy_body)
    == "{\"data\":{\"themeFilesCopy\":{\"copiedThemeFiles\":[{\"filename\":\"assets/copy.js\",\"checksumMd5\":\"6114f5adc373accd7b2051bd87078f62\",\"size\":14,\"body\":{\"content\":\"console.log(1)\"}}],\"userErrors\":[]}}}"

  let required_delete =
    "mutation { themeFilesDelete(themeId: \"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\", files: [\"config/settings_data.json\", \"config/settings_schema.json\"]) { deletedThemeFiles { filename } userErrors { field message code } } }"
  let #(
    Response(status: required_delete_status, body: required_delete_body, ..),
    proxy,
  ) = draft_proxy.process_request(proxy, graphql_request(required_delete))
  assert required_delete_status == 200
  assert json.to_string(required_delete_body)
    == "{\"data\":{\"themeFilesDelete\":{\"deletedThemeFiles\":[],\"userErrors\":[{\"field\":[\"files\",\"0\"],\"message\":\"File is required and can't be deleted\",\"code\":\"INVALID\"},{\"field\":[\"files\",\"1\"],\"message\":\"File is required and can't be deleted\",\"code\":\"INVALID\"}]}}}"

  let delete_copy =
    "mutation { themeFilesDelete(themeId: \"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\", files: [\"assets/copy.js\"]) { deletedThemeFiles { filename } userErrors { field message code } } }"
  let #(Response(status: delete_status, body: delete_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(delete_copy))
  assert delete_status == 200
  assert json.to_string(delete_body)
    == "{\"data\":{\"themeFilesDelete\":{\"deletedThemeFiles\":[{\"filename\":\"assets/copy.js\"}],\"userErrors\":[]}}}"

  let read =
    "query { theme(id: \"gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic\") { files(first: 10) { nodes { filename } } } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"theme\":{\"files\":{\"nodes\":[{\"filename\":\"assets/app.js\"}]}}}}"
}
