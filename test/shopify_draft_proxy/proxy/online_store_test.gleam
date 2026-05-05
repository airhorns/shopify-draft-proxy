import gleam/dict
import gleam/json
import gleam/list
import gleam/string
import shopify_draft_proxy/proxy/draft_proxy
import shopify_draft_proxy/proxy/proxy_state.{type Request, Request, Response}
import shopify_draft_proxy/state/store

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
