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
