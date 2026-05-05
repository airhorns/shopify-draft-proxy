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
import shopify_draft_proxy/state/types.{
  AccessScopeRecord, AppInstallationRecord, AppRecord,
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

fn run_graphql(proxy: DraftProxy, query: String) -> #(String, DraftProxy) {
  let #(Response(status: status, body: body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(query))
  assert status == 200
  #(json.to_string(body), proxy)
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
  assert log.status == store.Failed
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

pub fn web_pixel_create_without_settings_needs_configuration_test() {
  let query =
    "mutation { webPixelCreate(webPixel: {}) { webPixel { id status settings } userErrors { __typename code field message } } }"
  let #(body, _) = run_graphql(proxy(), query)
  assert body
    == "{\"data\":{\"webPixelCreate\":{\"webPixel\":{\"id\":\"gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic\",\"status\":\"NEEDS_CONFIGURATION\",\"settings\":null},\"userErrors\":[]}}}"
}

pub fn web_pixel_update_and_delete_errors_use_web_pixel_user_error_test() {
  let update_query =
    "mutation { webPixelUpdate(id: \"gid://shopify/WebPixel/missing\", webPixel: { settings: \"{}\" }) { webPixel { id } userErrors { __typename code field message } } }"
  let #(update_body, proxy) = run_graphql(proxy(), update_query)
  assert update_body
    == "{\"data\":{\"webPixelUpdate\":{\"webPixel\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":null,\"field\":[\"id\"],\"message\":\"Pixel does not exist\"}]}}}"

  let delete_query =
    "mutation { webPixelDelete(id: \"gid://shopify/WebPixel/missing\") { deletedWebPixelId userErrors { __typename code field message } } }"
  let #(delete_body, _) = run_graphql(proxy, delete_query)
  assert delete_body
    == "{\"data\":{\"webPixelDelete\":{\"deletedWebPixelId\":null,\"userErrors\":[{\"__typename\":\"WebPixelUserError\",\"code\":null,\"field\":[\"id\"],\"message\":\"Integration does not exist\"}]}}}"
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
  assert page_log.status == store.Failed
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
  assert blog_log.status == store.Failed
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
  assert blog_log.status == store.Staged
  assert article_log.status == store.Failed
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

pub fn page_body_html_is_scrubbed_on_create_update_and_read_test() {
  let proxy = draft_proxy.new()
  let create_query =
    "mutation { pageCreate(page: { title: \"Scrubbed Page\", body: \"<script>alert(1)</script><p onclick='alert(2)' class='safe'>Hi</p>\" }) { page { id body bodySummary } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"pageCreate\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"body\":\"<p class='safe'>Hi</p>\",\"bodySummary\":\"Hi\"},\"userErrors\":[]}}}"

  let read_after_create =
    "query { page(id: \"gid://shopify/Page/1?shopify-draft-proxy=synthetic\") { id body bodySummary } }"
  let #(Response(status: read_create_status, body: read_create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(read_after_create))
  assert read_create_status == 200
  assert json.to_string(read_create_body)
    == "{\"data\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"body\":\"<p class='safe'>Hi</p>\",\"bodySummary\":\"Hi\"}}}"

  let update_query =
    "mutation { pageUpdate(id: \"gid://shopify/Page/1?shopify-draft-proxy=synthetic\", page: { body: \"<div><script>outer<script>inner</script></script><iframe src='https://example.com/embed'>fallback</iframe><p onmouseover='bad'>After</p></div>\" }) { page { id body bodySummary } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"pageUpdate\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"body\":\"<div><p>After</p></div>\",\"bodySummary\":\"After\"},\"userErrors\":[]}}}"

  let read_after_update =
    "query { page(id: \"gid://shopify/Page/1?shopify-draft-proxy=synthetic\") { id body bodySummary } }"
  let #(Response(status: read_update_status, body: read_update_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_after_update))
  assert read_update_status == 200
  assert json.to_string(read_update_body)
    == "{\"data\":{\"page\":{\"id\":\"gid://shopify/Page/1?shopify-draft-proxy=synthetic\",\"body\":\"<div><p>After</p></div>\",\"bodySummary\":\"After\"}}}"
}

pub fn article_body_html_is_scrubbed_on_create_update_and_read_test() {
  let proxy = draft_proxy.new()
  let blog_query =
    "mutation { blogCreate(blog: { title: \"Scrubbed Blog\" }) { blog { id title } userErrors { field message code } } }"
  let #(Response(status: blog_status, body: blog_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(blog_query))
  assert blog_status == 200
  assert json.to_string(blog_body)
    == "{\"data\":{\"blogCreate\":{\"blog\":{\"id\":\"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\",\"title\":\"Scrubbed Blog\"},\"userErrors\":[]}}}"

  let create_query =
    "mutation { articleCreate(article: { title: \"Scrubbed Article\", body: \"<p onclick='bad'>Hi</p><script>alert(1)</script>\", blogId: \"gid://shopify/Blog/1?shopify-draft-proxy=synthetic\", author: { name: \"Scrubber\" } }) { article { id body summary } userErrors { field message code } } }"
  let #(Response(status: create_status, body: create_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(create_query))
  assert create_status == 200
  assert json.to_string(create_body)
    == "{\"data\":{\"articleCreate\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"body\":\"<p>Hi</p>\",\"summary\":\"\"},\"userErrors\":[]}}}"

  let update_query =
    "mutation { articleUpdate(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\", article: { body: \"<section><iframe src='x'></iframe><script>outer<script>inner</script></script><p onload='bad' data-ok='yes'>After</p></section>\" }) { article { id body } userErrors { field message code } } }"
  let #(Response(status: update_status, body: update_body, ..), proxy) =
    draft_proxy.process_request(proxy, graphql_request(update_query))
  assert update_status == 200
  assert json.to_string(update_body)
    == "{\"data\":{\"articleUpdate\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"body\":\"<section><p data-ok='yes'>After</p></section>\"},\"userErrors\":[]}}}"

  let read_after_update =
    "query { article(id: \"gid://shopify/Article/3?shopify-draft-proxy=synthetic\") { id body } }"
  let #(Response(status: read_status, body: read_body, ..), _) =
    draft_proxy.process_request(proxy, graphql_request(read_after_update))
  assert read_status == 200
  assert json.to_string(read_body)
    == "{\"data\":{\"article\":{\"id\":\"gid://shopify/Article/3?shopify-draft-proxy=synthetic\",\"body\":\"<section><p data-ok='yes'>After</p></section>\"}}}"
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
  assert missing_blog_log.status == store.Failed
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
  assert ambiguous_blog_log.status == store.Failed
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
  assert missing_author_log.status == store.Failed
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
  assert ambiguous_author_log.status == store.Failed
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
  assert blog_log.status == store.Staged
  assert article_log.status == store.Staged
  assert article_log.staged_resource_ids
    == ["gid://shopify/Article/3?shopify-draft-proxy=synthetic"]
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
