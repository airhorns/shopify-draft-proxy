import gleam/dict
import gleam/json
import gleam/list
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{
  type DraftProxy, type Request, Request, Response,
}
import shopify_draft_proxy/state/store

fn proxy() -> DraftProxy {
  draft_proxy.new()
  |> draft_proxy.with_default_registry()
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

fn read_state(proxy: DraftProxy) -> String {
  let #(Response(status: status, body: body, ..), _) =
    draft_proxy.process_request(proxy, meta_state_request())
  assert status == 200
  json.to_string(body)
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
