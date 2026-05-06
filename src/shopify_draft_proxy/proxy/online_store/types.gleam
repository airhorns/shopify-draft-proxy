//// Shared internal online-store constants.

@internal
pub const synthetic_shop_id: String = "gid://shopify/Shop/92891250994"

@internal
pub const online_store_blogs_count_query: String = "query OnlineStoreBlogsCountHydrate { blogsCount { count precision } }"

@internal
pub const online_store_pages_count_query: String = "query OnlineStorePagesCountHydrate { pagesCount { count precision } }"

@internal
pub const online_store_comment_hydrate_query: String = "query OnlineStoreCommentHydrate($id: ID!) { comment(id: $id) { __typename id status body bodyHtml isPublished publishedAt createdAt updatedAt article { id } } }"

pub const online_store_article_delete_cascade_hydrate_query: String = "query OnlineStoreArticleDeleteCascadeHydrate($id: ID!) { article(id: $id) { __typename id title handle createdAt updatedAt blog { id } comments(first: 50) { nodes { __typename id status body bodyHtml isPublished publishedAt createdAt updatedAt article { id } } } } }"

pub const online_store_blog_delete_cascade_hydrate_query: String = "query OnlineStoreBlogDeleteCascadeHydrate($id: ID!) { blog(id: $id) { __typename id title handle createdAt updatedAt commentPolicy articles(first: 50) { nodes { __typename id title handle createdAt updatedAt blog { id } comments(first: 50) { nodes { __typename id status body bodyHtml isPublished publishedAt createdAt updatedAt article { id } } } } } } }"
