use super::*;

mod online_store_helpers;
mod sales_channel;

pub(in crate::proxy) use self::online_store_helpers::*;

const ONLINE_STORE_CONTENT_TIMESTAMP: &str = "2024-01-01T00:00:00.000Z";
const ONLINE_STORE_TITLE_MAX_CHARS: usize = 255;
const ONLINE_STORE_HANDLE_MAX_CHARS: usize = 255;
const ONLINE_STORE_ARTICLE_HANDLE_MAX_CHARS: usize = 265;
const ONLINE_STORE_PAGE_BODY_MAX_BYTES: usize = 524_287;
const ONLINE_STORE_ARTICLE_BODY_MAX_BYTES: usize = 1_048_576;
const ONLINE_STORE_COMMENT_HYDRATE_QUERY: &str = "query OnlineStoreCommentHydrate($id: ID!) { comment(id: $id) { __typename id status body bodyHtml isPublished publishedAt createdAt updatedAt article { id } } }";
const ONLINE_STORE_PAGE_HYDRATE_QUERY: &str = "query OnlineStorePageHydrate($id: ID!) { page(id: $id) { __typename id title handle body bodySummary isPublished publishedAt createdAt updatedAt templateSuffix } }";
const ONLINE_STORE_ARTICLE_CASCADE_HYDRATE_QUERY: &str = "query OnlineStoreArticleDeleteCascadeHydrate($id: ID!) { article(id: $id) { __typename id title handle createdAt updatedAt blog { id } comments(first: 50) { nodes { __typename id status body bodyHtml isPublished publishedAt createdAt updatedAt article { id } } } } }";
const ONLINE_STORE_BLOG_CASCADE_HYDRATE_QUERY: &str = "query OnlineStoreBlogDeleteCascadeHydrate($id: ID!) { blog(id: $id) { __typename id title handle createdAt updatedAt commentPolicy articles(first: 50) { nodes { __typename id title handle createdAt updatedAt blog { id } comments(first: 50) { nodes { __typename id status body bodyHtml isPublished publishedAt createdAt updatedAt article { id } } } } } } }";
const ONLINE_STORE_BLOGS_COUNT_HYDRATE_QUERY: &str =
    "query OnlineStoreBlogsCountHydrate { blogsCount { count precision } }";
const ONLINE_STORE_PAGES_COUNT_HYDRATE_QUERY: &str =
    "query OnlineStorePagesCountHydrate { pagesCount { count precision } }";

impl DraftProxy {
    pub(in crate::proxy) fn online_store_content_query_value(
        &self,
        field: &RootFieldSelection,
    ) -> Option<Value> {
        match field.name.as_str() {
            "blog" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                Some(
                    self.online_store_blog_value(&id, &field.selection)
                        .unwrap_or(Value::Null),
                )
            }
            "blogs" => Some(selected_connection_json_with_args(
                self.online_store_blog_records(),
                &field.arguments,
                &field.selection,
                value_id_cursor,
            )),
            "blogsCount" => Some(selected_json(
                &publication_count_json(self.online_store_blog_count()),
                &field.selection,
            )),
            "page" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                Some(
                    self.online_store_page_value(&id, &field.selection)
                        .unwrap_or(Value::Null),
                )
            }
            "pages" => Some(selected_connection_json_with_args(
                self.online_store_page_records(),
                &field.arguments,
                &field.selection,
                value_id_cursor,
            )),
            "pagesCount" => Some(selected_json(
                &publication_count_json(self.online_store_page_count()),
                &field.selection,
            )),
            "article" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                Some(
                    self.online_store_article_value(&id, &field.selection)
                        .unwrap_or(Value::Null),
                )
            }
            "articles" => {
                let query = resolved_string_field(&field.arguments, "query");
                let mut records = self.online_store_article_records();
                records.retain(|article| article["isPublished"].as_bool().unwrap_or(false));
                if let Some(query) = query.as_deref() {
                    records.retain(|article| article_matches_query(article, query));
                }
                Some(selected_connection_json_with_args(
                    records,
                    &field.arguments,
                    &field.selection,
                    value_id_cursor,
                ))
            }
            "articleAuthors" => {
                let mut names = BTreeSet::new();
                for article in self.online_store_article_records() {
                    if let Some(name) = article["author"]["name"].as_str() {
                        if !name.is_empty() {
                            names.insert(name.to_string());
                        }
                    }
                }
                let records = names
                    .into_iter()
                    .map(|name| json!({ "name": name }))
                    .collect::<Vec<_>>();
                Some(selected_connection_json_with_args(
                    records,
                    &field.arguments,
                    &field.selection,
                    |_| String::new(),
                ))
            }
            "articleTags" => {
                let limit = resolved_int_field(&field.arguments, "limit")
                    .and_then(|limit| (limit >= 0).then_some(limit as usize));
                let mut tags = BTreeSet::new();
                for article in self.online_store_article_records() {
                    if let Some(values) = article["tags"].as_array() {
                        for tag in values {
                            if let Some(tag) = tag.as_str() {
                                tags.insert(tag.to_string());
                            }
                        }
                    }
                }
                let mut tags = tags.into_iter().map(Value::String).collect::<Vec<_>>();
                if let Some(limit) = limit {
                    tags.truncate(limit);
                }
                Some(Value::Array(tags))
            }
            "comment" => {
                let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
                Some(
                    self.online_store_comment_value(&id, &field.selection)
                        .unwrap_or(Value::Null),
                )
            }
            "comments" => Some(selected_connection_json_with_args(
                self.online_store_comment_records(),
                &field.arguments,
                &field.selection,
                value_id_cursor,
            )),
            _ => None,
        }
    }

    pub(in crate::proxy) fn online_store_content_mutation_value(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Option<Value> {
        match field.name.as_str() {
            "blogCreate" => Some(self.online_store_blog_create(field, staged_ids)),
            "blogUpdate" => Some(self.online_store_blog_update(field, request, staged_ids)),
            "blogDelete" => Some(self.online_store_blog_delete(field, request, staged_ids)),
            "pageCreate" => Some(self.online_store_page_create(field, staged_ids)),
            "pageUpdate" => Some(self.online_store_page_update(field, request, staged_ids)),
            "pageDelete" => Some(self.online_store_page_delete(field, request, staged_ids)),
            "articleCreate" => Some(self.online_store_article_create(field, staged_ids)),
            "articleUpdate" => Some(self.online_store_article_update(field, request, staged_ids)),
            "articleDelete" => Some(self.online_store_article_delete(field, request, staged_ids)),
            "commentApprove" => Some(self.online_store_comment_moderate(
                field,
                request,
                "commentApprove",
                staged_ids,
            )),
            "commentSpam" => {
                Some(self.online_store_comment_moderate(field, request, "commentSpam", staged_ids))
            }
            "commentNotSpam" => Some(self.online_store_comment_moderate(
                field,
                request,
                "commentNotSpam",
                staged_ids,
            )),
            "commentDelete" => Some(self.online_store_comment_delete(field, request, staged_ids)),
            _ => None,
        }
    }

    pub(in crate::proxy) fn online_store_content_node_value(
        &self,
        id: &str,
        selection: &[SelectedField],
    ) -> Option<Value> {
        match shopify_gid_resource_type(id) {
            Some("Blog") => self.online_store_blog_value(id, selection),
            Some("Page") => self.online_store_page_value(id, selection),
            Some("Article") => self.online_store_article_value(id, selection),
            Some("Comment") => self.online_store_comment_value(id, selection),
            _ => None,
        }
    }

    pub(in crate::proxy) fn online_store_content_query_needs_upstream(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        if self.config.read_mode == ReadMode::Snapshot {
            return false;
        }
        let has_content_root = fields
            .iter()
            .any(|field| is_online_store_content_query_root(&field.name));
        has_content_root
            && fields
                .iter()
                .all(|field| is_online_store_content_query_root(&field.name))
            && !self.has_online_store_content_state()
    }

    pub(in crate::proxy) fn hydrate_online_store_content_query_baselines(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) {
        if self.config.read_mode == ReadMode::Snapshot {
            return;
        }
        if fields.iter().any(|field| field.name == "blogsCount")
            && self.store.staged.online_store_blogs_count_base.is_none()
        {
            self.hydrate_online_store_count_base(
                request,
                "blogsCount",
                ONLINE_STORE_BLOGS_COUNT_HYDRATE_QUERY,
            );
        }
        if fields.iter().any(|field| field.name == "pagesCount")
            && self.store.staged.online_store_pages_count_base.is_none()
        {
            self.hydrate_online_store_count_base(
                request,
                "pagesCount",
                ONLINE_STORE_PAGES_COUNT_HYDRATE_QUERY,
            );
        }
    }

    pub(in crate::proxy) fn observe_online_store_content_response(&mut self, body: &Value) {
        let Some(data) = body.get("data") else {
            return;
        };
        if let Some(count) = data
            .get("blogsCount")
            .and_then(|value| value.get("count"))
            .and_then(Value::as_u64)
        {
            self.store.staged.online_store_blogs_count_base = Some(count as usize);
        }
        if let Some(count) = data
            .get("pagesCount")
            .and_then(|value| value.get("count"))
            .and_then(Value::as_u64)
        {
            self.store.staged.online_store_pages_count_base = Some(count as usize);
        }
        self.observe_online_store_content_node(data, None, None);
    }

    fn has_online_store_content_state(&self) -> bool {
        !self.store.staged.online_store_blogs.is_empty()
            || !self.store.staged.online_store_pages.is_empty()
            || !self.store.staged.online_store_articles.is_empty()
            || !self.store.staged.online_store_comments.is_empty()
            || !self.store.staged.deleted_online_store_blog_ids.is_empty()
            || !self.store.staged.deleted_online_store_page_ids.is_empty()
            || !self
                .store
                .staged
                .deleted_online_store_article_ids
                .is_empty()
            || !self
                .store
                .staged
                .deleted_online_store_comment_ids
                .is_empty()
            || self.store.staged.online_store_blogs_count_base.is_some()
            || self.store.staged.online_store_pages_count_base.is_some()
    }

    fn hydrate_online_store_count_base(&mut self, request: &Request, root: &str, query: &str) {
        let response = self.upstream_post(request, json!({ "query": query, "variables": {} }));
        if response.status >= 400 {
            return;
        }
        if let Some(count) = response
            .body
            .get("data")
            .and_then(|data| data.get(root))
            .and_then(|value| value.get("count"))
            .and_then(Value::as_u64)
        {
            match root {
                "blogsCount" => {
                    self.store.staged.online_store_blogs_count_base = Some(count as usize)
                }
                "pagesCount" => {
                    self.store.staged.online_store_pages_count_base = Some(count as usize)
                }
                _ => {}
            }
        }
    }

    fn hydrate_online_store_content_from_upstream(
        &mut self,
        request: &Request,
        id: &str,
        query: &str,
    ) {
        if self.config.read_mode == ReadMode::Snapshot || id.is_empty() {
            return;
        }
        let response = self.upstream_post(
            request,
            json!({
                "query": query,
                "variables": { "id": id }
            }),
        );
        if response.status < 400 {
            self.observe_online_store_content_response(&response.body);
        }
    }

    fn observe_online_store_content_node(
        &mut self,
        node: &Value,
        parent_blog_id: Option<String>,
        parent_article_id: Option<String>,
    ) {
        match node {
            Value::Array(entries) => {
                for entry in entries {
                    self.observe_online_store_content_node(
                        entry,
                        parent_blog_id.clone(),
                        parent_article_id.clone(),
                    );
                }
            }
            Value::Object(object) => {
                let mut next_parent_blog_id = parent_blog_id.clone();
                let mut next_parent_article_id = parent_article_id.clone();
                if let Some(id) = object.get("id").and_then(Value::as_str) {
                    match shopify_gid_resource_type(id) {
                        Some("Blog") if should_stage_observed_blog(node) => {
                            self.stage_online_store_blog(
                                id.to_string(),
                                normalize_observed_blog(node),
                            );
                            next_parent_blog_id = Some(id.to_string());
                        }
                        Some("Page") if should_stage_observed_page(node) => {
                            self.stage_online_store_page(
                                id.to_string(),
                                normalize_observed_page(node),
                            );
                        }
                        Some("Article") if should_stage_observed_article(node) => {
                            self.stage_online_store_article(
                                id.to_string(),
                                normalize_observed_article(node, parent_blog_id.as_deref()),
                            );
                            next_parent_article_id = Some(id.to_string());
                        }
                        Some("Comment") if should_stage_observed_comment(node) => {
                            self.stage_online_store_comment(
                                id.to_string(),
                                normalize_observed_comment(node, parent_article_id.as_deref()),
                            );
                        }
                        _ => {}
                    }
                }
                for value in object.values() {
                    self.observe_online_store_content_node(
                        value,
                        next_parent_blog_id.clone(),
                        next_parent_article_id.clone(),
                    );
                }
            }
            _ => {}
        }
    }

    fn online_store_blog_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let Some(input) = resolved_object_field(&field.arguments, "blog") else {
            return online_store_payload(
                &field.selection,
                "blog",
                Value::Null,
                vec![user_error(
                    vec!["blog", "title"],
                    "Title can't be blank",
                    None,
                )],
            );
        };
        if let Some(error) = title_blank_error(&input, "blog", None, true) {
            return online_store_payload(&field.selection, "blog", Value::Null, vec![error]);
        }
        if let Some(error) =
            content_length_error(&input, "blog", ONLINE_STORE_HANDLE_MAX_CHARS, None)
        {
            return online_store_payload(&field.selection, "blog", Value::Null, vec![error]);
        }
        if let Some(error) = commentable_inclusion_error(&input) {
            return online_store_payload(&field.selection, "blog", Value::Null, vec![error]);
        }
        let id = self.next_online_store_id("Blog");
        let record = blog_record(&id, &input);
        self.stage_online_store_blog(id.clone(), record.clone());
        staged_ids.push(id);
        online_store_payload(
            &field.selection,
            "blog",
            self.enriched_blog_record(&record),
            Vec::new(),
        )
    }

    fn online_store_blog_update(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.online_store_blogs.contains_key(&id) {
            self.hydrate_online_store_content_from_upstream(
                request,
                &id,
                ONLINE_STORE_BLOG_CASCADE_HYDRATE_QUERY,
            );
        }
        let Some(mut record) = self
            .store
            .staged
            .online_store_blogs
            .get(&id)
            .cloned()
            .filter(|_| {
                !self
                    .store
                    .staged
                    .deleted_online_store_blog_ids
                    .contains(&id)
            })
        else {
            return online_store_payload(
                &field.selection,
                "blog",
                Value::Null,
                vec![user_error(
                    vec!["id"],
                    "Blog does not exist",
                    Some("NOT_FOUND"),
                )],
            );
        };
        let input = resolved_object_field(&field.arguments, "blog").unwrap_or_default();
        if let Some(error) = title_blank_error(&input, "blog", None, false) {
            return online_store_payload(&field.selection, "blog", Value::Null, vec![error]);
        }
        if let Some(error) =
            content_length_error(&input, "blog", ONLINE_STORE_HANDLE_MAX_CHARS, None)
        {
            return online_store_payload(&field.selection, "blog", Value::Null, vec![error]);
        }
        if let Some(error) = commentable_inclusion_error(&input) {
            return online_store_payload(&field.selection, "blog", Value::Null, vec![error]);
        }
        apply_blog_input(&mut record, &input, false);
        self.stage_online_store_blog(id.clone(), record.clone());
        staged_ids.push(id);
        online_store_payload(
            &field.selection,
            "blog",
            self.enriched_blog_record(&record),
            Vec::new(),
        )
    }

    fn online_store_blog_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.online_store_blogs.contains_key(&id) {
            self.hydrate_online_store_content_from_upstream(
                request,
                &id,
                ONLINE_STORE_BLOG_CASCADE_HYDRATE_QUERY,
            );
        }
        if !self.store.staged.online_store_blogs.contains_key(&id)
            || self
                .store
                .staged
                .deleted_online_store_blog_ids
                .contains(&id)
        {
            return delete_payload(
                &field.selection,
                "deletedBlogId",
                Value::Null,
                vec![user_error(
                    vec!["id"],
                    "Blog does not exist",
                    Some("NOT_FOUND"),
                )],
            );
        }
        self.store
            .staged
            .deleted_online_store_blog_ids
            .insert(id.clone());
        let article_ids = self
            .store
            .staged
            .online_store_articles
            .values()
            .filter(|article| article["blogId"].as_str() == Some(id.as_str()))
            .filter_map(|article| article["id"].as_str().map(str::to_string))
            .collect::<Vec<_>>();
        for article_id in article_ids {
            self.tombstone_online_store_article(&article_id);
        }
        staged_ids.push(id.clone());
        delete_payload(&field.selection, "deletedBlogId", json!(id), Vec::new())
    }

    fn online_store_page_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "page").unwrap_or_default();
        if let Some(error) = title_blank_error(&input, "page", Some("BLANK"), true) {
            return online_store_payload(&field.selection, "page", Value::Null, vec![error]);
        }
        if let Some(error) = content_length_error(
            &input,
            "page",
            ONLINE_STORE_HANDLE_MAX_CHARS,
            Some((
                ONLINE_STORE_PAGE_BODY_MAX_BYTES,
                "Content is too big (maximum is 512 KB)",
                Some("TOO_BIG"),
            )),
        ) {
            return online_store_payload(&field.selection, "page", Value::Null, vec![error]);
        }
        if let Some(handle) = resolved_string_field(&input, "handle") {
            if self.online_store_page_handle_taken(&handle, None) {
                return online_store_payload(
                    &field.selection,
                    "page",
                    Value::Null,
                    vec![user_error(
                        vec!["page", "handle"],
                        "Handle has already been taken",
                        Some("TAKEN"),
                    )],
                );
            }
        }
        let effective_published = resolved_bool_field(&input, "isPublished")
            .or_else(|| resolved_bool_field(&input, "visible"))
            .unwrap_or(true);
        if let Some(error) = invalid_publish_date_error(&input, "page", effective_published) {
            return online_store_payload(&field.selection, "page", Value::Null, vec![error]);
        }
        let id = self.next_online_store_id("Page");
        let mut record = page_record(&id, &input, None);
        if !input.contains_key("handle") {
            let handle = record["handle"].as_str().unwrap_or_default();
            record["handle"] = json!(self.unique_online_store_page_handle(handle, None));
        }
        self.stage_online_store_page(id.clone(), record.clone());
        staged_ids.push(id);
        online_store_payload(&field.selection, "page", record, Vec::new())
    }

    fn online_store_page_update(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.online_store_pages.contains_key(&id) {
            self.hydrate_online_store_content_from_upstream(
                request,
                &id,
                ONLINE_STORE_PAGE_HYDRATE_QUERY,
            );
        }
        let Some(mut record) = self
            .store
            .staged
            .online_store_pages
            .get(&id)
            .cloned()
            .filter(|_| {
                !self
                    .store
                    .staged
                    .deleted_online_store_page_ids
                    .contains(&id)
            })
        else {
            return online_store_payload(
                &field.selection,
                "page",
                Value::Null,
                vec![user_error(
                    vec!["id"],
                    "Page does not exist",
                    Some("NOT_FOUND"),
                )],
            );
        };
        let input = resolved_object_field(&field.arguments, "page").unwrap_or_default();
        if let Some(error) = title_blank_error(&input, "page", Some("BLANK"), false) {
            return online_store_payload(&field.selection, "page", Value::Null, vec![error]);
        }
        if let Some(error) = content_length_error(
            &input,
            "page",
            ONLINE_STORE_HANDLE_MAX_CHARS,
            Some((
                ONLINE_STORE_PAGE_BODY_MAX_BYTES,
                "Content is too big (maximum is 512 KB)",
                Some("TOO_BIG"),
            )),
        ) {
            return online_store_payload(&field.selection, "page", Value::Null, vec![error]);
        }
        if let Some(handle) = resolved_string_field(&input, "handle") {
            if self.online_store_page_handle_taken(&handle, Some(id.as_str())) {
                return online_store_payload(
                    &field.selection,
                    "page",
                    Value::Null,
                    vec![user_error(
                        vec!["page", "handle"],
                        "Handle has already been taken",
                        Some("TAKEN"),
                    )],
                );
            }
        }
        let effective_published = resolved_bool_field(&input, "isPublished")
            .or_else(|| resolved_bool_field(&input, "visible"))
            .or_else(|| record["isPublished"].as_bool())
            .unwrap_or(false);
        if let Some(error) = invalid_publish_date_error(&input, "page", effective_published) {
            return online_store_payload(&field.selection, "page", Value::Null, vec![error]);
        }
        apply_page_input(&mut record, &input);
        if input.contains_key("title") && !input.contains_key("handle") {
            let handle = record["handle"].as_str().unwrap_or_default();
            record["handle"] =
                json!(self.unique_online_store_page_handle(handle, Some(id.as_str())));
        }
        self.stage_online_store_page(id.clone(), record.clone());
        staged_ids.push(id);
        online_store_payload(&field.selection, "page", record, Vec::new())
    }

    fn online_store_page_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.online_store_pages.contains_key(&id) {
            self.hydrate_online_store_content_from_upstream(
                request,
                &id,
                ONLINE_STORE_PAGE_HYDRATE_QUERY,
            );
        }
        if !self.store.staged.online_store_pages.contains_key(&id)
            || self
                .store
                .staged
                .deleted_online_store_page_ids
                .contains(&id)
        {
            return delete_payload(
                &field.selection,
                "deletedPageId",
                Value::Null,
                vec![user_error(
                    vec!["id"],
                    "Page does not exist",
                    Some("NOT_FOUND"),
                )],
            );
        }
        self.store
            .staged
            .deleted_online_store_page_ids
            .insert(id.clone());
        staged_ids.push(id.clone());
        delete_payload(&field.selection, "deletedPageId", json!(id), Vec::new())
    }

    fn online_store_article_create(
        &mut self,
        field: &RootFieldSelection,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let input = resolved_object_field(&field.arguments, "article").unwrap_or_default();
        if let Some(error) = title_blank_error(&input, "article", Some("BLANK"), true) {
            return online_store_payload(&field.selection, "article", Value::Null, vec![error]);
        }
        if let Some(error) = content_length_error(
            &input,
            "article",
            ONLINE_STORE_ARTICLE_HANDLE_MAX_CHARS,
            Some((
                ONLINE_STORE_ARTICLE_BODY_MAX_BYTES,
                "Content is too big (maximum is 1 MB)",
                None,
            )),
        ) {
            return online_store_payload(&field.selection, "article", Value::Null, vec![error]);
        }
        let effective_published = resolved_bool_field(&input, "isPublished")
            .or_else(|| resolved_bool_field(&input, "visible"))
            .unwrap_or(true);
        if let Some(error) = invalid_publish_date_error(&input, "article", effective_published) {
            return online_store_payload(&field.selection, "article", Value::Null, vec![error]);
        }
        let inline_blog = resolved_object_field(&field.arguments, "blog");
        let blog_id = resolved_string_field(&input, "blogId");
        if blog_id.is_some() && inline_blog.is_some() {
            return online_store_payload(
                &field.selection,
                "article",
                Value::Null,
                vec![user_error(
                    vec!["article"],
                    "Can't create a blog from input if a blog ID is supplied.",
                    Some("AMBIGUOUS_BLOG"),
                )],
            );
        }
        let blog_id = if let Some(blog_id) = blog_id {
            if self.online_store_blog_record(&blog_id).is_none() {
                return article_blog_not_found_payload(&field.selection, "article");
            }
            blog_id
        } else if let Some(blog) = inline_blog {
            if let Some(error) = title_blank_error(&blog, "blog", None, true) {
                return online_store_payload(&field.selection, "article", Value::Null, vec![error]);
            }
            let id = self.next_online_store_id("Blog");
            let record = blog_record(&id, &blog);
            self.stage_online_store_blog(id.clone(), record);
            staged_ids.push(id.clone());
            id
        } else {
            return online_store_payload(
                &field.selection,
                "article",
                Value::Null,
                vec![user_error(
                    vec!["article"],
                    "Must reference or create a blog when creating an article.",
                    Some("BLOG_REFERENCE_REQUIRED"),
                )],
            );
        };
        if let Some(error) = article_author_error(&input, true, true) {
            return online_store_payload(&field.selection, "article", Value::Null, vec![error]);
        }
        let id = self.next_online_store_id("Article");
        let record = article_record(&id, &blog_id, &input, None);
        self.stage_online_store_article(id.clone(), record.clone());
        staged_ids.push(id);
        online_store_payload(
            &field.selection,
            "article",
            self.enriched_article_record(&record),
            Vec::new(),
        )
    }

    fn online_store_article_update(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.online_store_articles.contains_key(&id) {
            self.hydrate_online_store_content_from_upstream(
                request,
                &id,
                ONLINE_STORE_ARTICLE_CASCADE_HYDRATE_QUERY,
            );
        }
        let Some(mut record) = self
            .store
            .staged
            .online_store_articles
            .get(&id)
            .cloned()
            .filter(|_| {
                !self
                    .store
                    .staged
                    .deleted_online_store_article_ids
                    .contains(&id)
            })
        else {
            return online_store_payload(
                &field.selection,
                "article",
                Value::Null,
                vec![user_error(
                    vec!["id"],
                    "Article does not exist",
                    Some("NOT_FOUND"),
                )],
            );
        };
        let input = resolved_object_field(&field.arguments, "article").unwrap_or_default();
        if let Some(error) = title_blank_error(&input, "article", Some("BLANK"), false) {
            return online_store_payload(&field.selection, "article", Value::Null, vec![error]);
        }
        if let Some(error) = content_length_error(
            &input,
            "article",
            ONLINE_STORE_ARTICLE_HANDLE_MAX_CHARS,
            Some((
                ONLINE_STORE_ARTICLE_BODY_MAX_BYTES,
                "Content is too big (maximum is 1 MB)",
                None,
            )),
        ) {
            return online_store_payload(&field.selection, "article", Value::Null, vec![error]);
        }
        let effective_published = resolved_bool_field(&input, "isPublished")
            .or_else(|| resolved_bool_field(&input, "visible"))
            .or_else(|| record["isPublished"].as_bool())
            .unwrap_or(false);
        if let Some(error) = invalid_publish_date_error(&input, "article", effective_published) {
            return online_store_payload(&field.selection, "article", Value::Null, vec![error]);
        }
        if let Some(blog_id) = resolved_string_field(&input, "blogId") {
            if self.online_store_blog_record(&blog_id).is_none() {
                return article_blog_not_found_payload(&field.selection, "article");
            }
            record["blogId"] = json!(blog_id);
        }
        if let Some(error) = article_author_error(&input, false, false) {
            return online_store_payload(&field.selection, "article", Value::Null, vec![error]);
        }
        if let Some(error) = article_image_update_error(&record, &input) {
            return online_store_payload(&field.selection, "article", Value::Null, vec![error]);
        }
        apply_article_input(&mut record, &input);
        self.stage_online_store_article(id.clone(), record.clone());
        staged_ids.push(id);
        online_store_payload(
            &field.selection,
            "article",
            self.enriched_article_record(&record),
            Vec::new(),
        )
    }

    fn online_store_article_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.online_store_articles.contains_key(&id) {
            self.hydrate_online_store_content_from_upstream(
                request,
                &id,
                ONLINE_STORE_ARTICLE_CASCADE_HYDRATE_QUERY,
            );
        }
        if !self.store.staged.online_store_articles.contains_key(&id)
            || self
                .store
                .staged
                .deleted_online_store_article_ids
                .contains(&id)
        {
            return delete_payload(
                &field.selection,
                "deletedArticleId",
                Value::Null,
                vec![user_error(
                    vec!["id"],
                    "Article does not exist",
                    Some("NOT_FOUND"),
                )],
            );
        }
        self.tombstone_online_store_article(&id);
        staged_ids.push(id.clone());
        delete_payload(&field.selection, "deletedArticleId", json!(id), Vec::new())
    }

    fn online_store_comment_moderate(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        root: &str,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.online_store_comments.contains_key(&id) {
            self.hydrate_online_store_content_from_upstream(
                request,
                &id,
                ONLINE_STORE_COMMENT_HYDRATE_QUERY,
            );
        }
        let Some(mut comment) = self
            .store
            .staged
            .online_store_comments
            .get(&id)
            .cloned()
            .filter(|_| {
                !self
                    .store
                    .staged
                    .deleted_online_store_comment_ids
                    .contains(&id)
            })
        else {
            return online_store_payload(
                &field.selection,
                "comment",
                Value::Null,
                vec![comment_not_found_error()],
            );
        };
        let status = comment["status"]
            .as_str()
            .unwrap_or("UNAPPROVED")
            .to_string();
        let transition = match root {
            "commentApprove" => match status.as_str() {
                "PUBLISHED" => Ok("PUBLISHED"),
                "UNAPPROVED" | "PENDING" => Ok("PUBLISHED"),
                "SPAM" => Err("Status cannot transition via \"approve\""),
                _ => Err("Status cannot transition via \"approve\""),
            },
            "commentSpam" => match status.as_str() {
                "SPAM" => Ok("SPAM"),
                "PUBLISHED" | "UNAPPROVED" | "PENDING" => Ok("SPAM"),
                _ => Err("Status cannot transition via \"spam\""),
            },
            "commentNotSpam" => match status.as_str() {
                "PUBLISHED" => Ok("PUBLISHED"),
                "SPAM" => Ok("PUBLISHED"),
                "UNAPPROVED" | "PENDING" => Err("Status cannot transition via \"not spam\""),
                _ => Err("Status cannot transition via \"not spam\""),
            },
            _ => Ok(status.as_str()),
        };
        let next_status = match transition {
            Ok(next_status) => next_status,
            Err(message) => {
                return online_store_payload(
                    &field.selection,
                    "comment",
                    Value::Null,
                    vec![user_error(vec!["id"], message, None)],
                )
            }
        };
        let changed = status != next_status;
        comment["status"] = json!(next_status);
        comment["isPublished"] = json!(next_status == "PUBLISHED");
        if next_status == "PUBLISHED" && comment["publishedAt"].is_null() {
            comment["publishedAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
        } else if next_status != "PUBLISHED" {
            comment["publishedAt"] = Value::Null;
        }
        if changed {
            comment["updatedAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
            self.stage_online_store_comment(id.clone(), comment.clone());
            staged_ids.push(id);
        }
        online_store_payload(
            &field.selection,
            "comment",
            self.enriched_comment_record(&comment),
            Vec::new(),
        )
    }

    fn online_store_comment_delete(
        &mut self,
        field: &RootFieldSelection,
        request: &Request,
        staged_ids: &mut Vec<String>,
    ) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        if !self.store.staged.online_store_comments.contains_key(&id) {
            self.hydrate_online_store_content_from_upstream(
                request,
                &id,
                ONLINE_STORE_COMMENT_HYDRATE_QUERY,
            );
        }
        if self
            .store
            .staged
            .deleted_online_store_comment_ids
            .contains(&id)
            || !self.store.staged.online_store_comments.contains_key(&id)
        {
            return delete_payload(
                &field.selection,
                "deletedCommentId",
                Value::Null,
                vec![comment_not_found_error()],
            );
        }
        if let Some(article_id) = self
            .store
            .staged
            .online_store_comments
            .get(&id)
            .and_then(|comment| string_value(comment, "articleId"))
        {
            if !article_id.is_empty()
                && !self
                    .store
                    .staged
                    .online_store_articles
                    .contains_key(&article_id)
                && !self
                    .store
                    .staged
                    .deleted_online_store_article_ids
                    .contains(&article_id)
            {
                self.hydrate_online_store_content_from_upstream(
                    request,
                    &article_id,
                    ONLINE_STORE_ARTICLE_CASCADE_HYDRATE_QUERY,
                );
            }
        }
        self.store.staged.online_store_comments.remove(&id);
        self.store
            .staged
            .online_store_comment_order
            .retain(|comment_id| comment_id != &id);
        self.store
            .staged
            .deleted_online_store_comment_ids
            .insert(id.clone());
        staged_ids.push(id.clone());
        delete_payload(&field.selection, "deletedCommentId", json!(id), Vec::new())
    }

    fn online_store_blog_value(&self, id: &str, selection: &[SelectedField]) -> Option<Value> {
        self.online_store_blog_record(id)
            .map(|record| selected_json(&self.enriched_blog_record(&record), selection))
    }

    fn online_store_page_value(&self, id: &str, selection: &[SelectedField]) -> Option<Value> {
        self.online_store_page_record(id)
            .map(|record| selected_json(&record, selection))
    }

    fn online_store_article_value(&self, id: &str, selection: &[SelectedField]) -> Option<Value> {
        self.online_store_article_record(id)
            .map(|record| selected_json(&self.enriched_article_record(&record), selection))
    }

    fn online_store_comment_value(&self, id: &str, selection: &[SelectedField]) -> Option<Value> {
        self.online_store_comment_record(id)
            .map(|record| selected_json(&self.enriched_comment_record(&record), selection))
    }

    fn online_store_blog_record(&self, id: &str) -> Option<Value> {
        (!self.store.staged.deleted_online_store_blog_ids.contains(id))
            .then(|| self.store.staged.online_store_blogs.get(id).cloned())
            .flatten()
    }

    fn online_store_page_record(&self, id: &str) -> Option<Value> {
        (!self.store.staged.deleted_online_store_page_ids.contains(id))
            .then(|| self.store.staged.online_store_pages.get(id).cloned())
            .flatten()
    }

    fn online_store_article_record(&self, id: &str) -> Option<Value> {
        (!self
            .store
            .staged
            .deleted_online_store_article_ids
            .contains(id))
        .then(|| self.store.staged.online_store_articles.get(id).cloned())
        .flatten()
    }

    fn online_store_comment_record(&self, id: &str) -> Option<Value> {
        (!self
            .store
            .staged
            .deleted_online_store_comment_ids
            .contains(id))
        .then(|| self.store.staged.online_store_comments.get(id).cloned())
        .flatten()
    }

    fn online_store_blog_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_blog_order
            .iter()
            .filter_map(|id| self.online_store_blog_record(id))
            .map(|record| self.enriched_blog_record(&record))
            .collect()
    }

    fn online_store_blog_count(&self) -> usize {
        online_store_count_with_baseline(
            self.store.staged.online_store_blogs_count_base,
            &self.store.staged.online_store_blog_order,
            &self.store.staged.deleted_online_store_blog_ids,
        )
        .unwrap_or_else(|| self.online_store_blog_records().len())
    }

    fn online_store_page_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_page_order
            .iter()
            .filter_map(|id| self.online_store_page_record(id))
            .collect()
    }

    fn online_store_page_count(&self) -> usize {
        online_store_count_with_baseline(
            self.store.staged.online_store_pages_count_base,
            &self.store.staged.online_store_page_order,
            &self.store.staged.deleted_online_store_page_ids,
        )
        .unwrap_or_else(|| self.online_store_page_records().len())
    }

    fn online_store_article_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_article_order
            .iter()
            .filter_map(|id| self.online_store_article_record(id))
            .map(|record| self.enriched_article_record(&record))
            .collect()
    }

    fn online_store_comment_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_comment_order
            .iter()
            .filter_map(|id| self.online_store_comment_record(id))
            .map(|record| self.enriched_comment_record(&record))
            .collect()
    }

    fn enriched_blog_record(&self, record: &Value) -> Value {
        let mut record = record.clone();
        let id = record["id"].as_str().unwrap_or_default();
        let articles = self
            .online_store_article_records()
            .into_iter()
            .filter(|article| article["blogId"].as_str() == Some(id))
            .collect::<Vec<_>>();
        record["articlesCount"] = publication_count_json(articles.len());
        record["articles"] = connection_json(articles);
        record
    }

    fn enriched_article_record(&self, record: &Value) -> Value {
        let mut record = record.clone();
        let article_id = record["id"].as_str().unwrap_or_default().to_string();
        let blog_id = record["blogId"].as_str().unwrap_or_default().to_string();
        record["blog"] = self
            .online_store_blog_record(&blog_id)
            .map(|blog| {
                json!({
                    "__typename": "Blog",
                    "id": blog["id"].clone(),
                    "title": blog["title"].clone(),
                    "handle": blog["handle"].clone()
                })
            })
            .unwrap_or(Value::Null);
        let comments = self
            .online_store_comment_records()
            .into_iter()
            .filter(|comment| comment["articleId"].as_str() == Some(article_id.as_str()))
            .collect::<Vec<_>>();
        record["commentsCount"] = publication_count_json(comments.len());
        record["comments"] = connection_json(comments);
        record
    }

    fn enriched_comment_record(&self, record: &Value) -> Value {
        let mut record = record.clone();
        let article_id = record["articleId"].as_str().unwrap_or_default();
        record["article"] = self
            .online_store_article_record(article_id)
            .map(|article| {
                json!({
                    "__typename": "Article",
                    "id": article["id"].clone(),
                    "title": article["title"].clone()
                })
            })
            .unwrap_or(Value::Null);
        record
    }

    fn stage_online_store_blog(&mut self, id: String, record: Value) {
        self.store.staged.deleted_online_store_blog_ids.remove(&id);
        if !self.store.staged.online_store_blogs.contains_key(&id) {
            self.store.staged.online_store_blog_order.push(id.clone());
        }
        self.store.staged.online_store_blogs.insert(id, record);
    }

    fn stage_online_store_page(&mut self, id: String, record: Value) {
        self.store.staged.deleted_online_store_page_ids.remove(&id);
        if !self.store.staged.online_store_pages.contains_key(&id) {
            self.store.staged.online_store_page_order.push(id.clone());
        }
        self.store.staged.online_store_pages.insert(id, record);
    }

    fn stage_online_store_article(&mut self, id: String, record: Value) {
        self.store
            .staged
            .deleted_online_store_article_ids
            .remove(&id);
        if !self.store.staged.online_store_articles.contains_key(&id) {
            self.store
                .staged
                .online_store_article_order
                .push(id.clone());
        }
        self.store.staged.online_store_articles.insert(id, record);
    }

    fn stage_online_store_comment(&mut self, id: String, record: Value) {
        self.store
            .staged
            .deleted_online_store_comment_ids
            .remove(&id);
        if !self.store.staged.online_store_comments.contains_key(&id) {
            self.store
                .staged
                .online_store_comment_order
                .push(id.clone());
        }
        self.store.staged.online_store_comments.insert(id, record);
    }

    fn tombstone_online_store_article(&mut self, id: &str) {
        self.store
            .staged
            .deleted_online_store_article_ids
            .insert(id.to_string());
        let comment_ids = self
            .store
            .staged
            .online_store_comments
            .values()
            .filter(|comment| comment["articleId"].as_str() == Some(id))
            .filter_map(|comment| comment["id"].as_str().map(str::to_string))
            .collect::<Vec<_>>();
        for comment_id in comment_ids {
            self.store.staged.online_store_comments.remove(&comment_id);
            self.store
                .staged
                .online_store_comment_order
                .retain(|id| id != &comment_id);
            self.store
                .staged
                .deleted_online_store_comment_ids
                .insert(comment_id);
        }
    }

    fn online_store_page_handle_taken(&self, handle: &str, excluding_id: Option<&str>) -> bool {
        self.store
            .staged
            .online_store_pages
            .values()
            .filter(|page| page["id"].as_str() != excluding_id)
            .filter(|page| {
                !page["id"]
                    .as_str()
                    .is_some_and(|id| self.store.staged.deleted_online_store_page_ids.contains(id))
            })
            .any(|page| page["handle"].as_str() == Some(handle))
    }

    fn unique_online_store_page_handle(&self, base: &str, excluding_id: Option<&str>) -> String {
        if !self.online_store_page_handle_taken(base, excluding_id) {
            return base.to_string();
        }
        for index in 1.. {
            let candidate = format!("{base}-{index}");
            if !self.online_store_page_handle_taken(&candidate, excluding_id) {
                return candidate;
            }
        }
        unreachable!("unbounded page handle suffix search should return")
    }
}

fn blog_record(id: &str, input: &BTreeMap<String, ResolvedValue>) -> Value {
    let title = resolved_string_field(input, "title").unwrap_or_default();
    let handle = resolved_string_field(input, "handle").unwrap_or_else(|| slugify_handle(&title));
    let comment_policy = resolved_string_field(input, "commentPolicy")
        .or_else(|| {
            resolved_string_field(input, "commentable").map(|value| {
                if value == "MODERATE" {
                    "MODERATED".to_string()
                } else {
                    value
                }
            })
        })
        .unwrap_or_else(|| "CLOSED".to_string());
    json!({
        "__typename": "Blog",
        "id": id,
        "title": title,
        "handle": handle,
        "commentPolicy": comment_policy,
        "tags": resolved_string_list_field(input, "tags"),
        "templateSuffix": optional_string_value(input, "templateSuffix"),
        "createdAt": ONLINE_STORE_CONTENT_TIMESTAMP,
        "updatedAt": ONLINE_STORE_CONTENT_TIMESTAMP,
        "articlesCount": publication_count_json(0),
        "articles": connection_json(Vec::new())
    })
}

fn apply_blog_input(record: &mut Value, input: &BTreeMap<String, ResolvedValue>, create: bool) {
    if let Some(title) = resolved_string_field(input, "title") {
        record["title"] = json!(title);
        if create || !input.contains_key("handle") {
            record["handle"] = json!(slugify_handle(record["title"].as_str().unwrap_or_default()));
        }
    }
    if let Some(handle) = resolved_string_field(input, "handle") {
        record["handle"] = json!(handle);
    }
    if let Some(comment_policy) = resolved_string_field(input, "commentPolicy") {
        record["commentPolicy"] = json!(comment_policy);
    }
    if let Some(commentable) = resolved_string_field(input, "commentable") {
        record["commentPolicy"] = json!(if commentable == "MODERATE" {
            "MODERATED".to_string()
        } else {
            commentable
        });
    }
    if input.contains_key("tags") {
        record["tags"] = json!(resolved_string_list_field(input, "tags"));
    }
    if input.contains_key("templateSuffix") {
        record["templateSuffix"] = optional_string_value(input, "templateSuffix");
    }
    record["updatedAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
}

fn page_record(
    id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
) -> Value {
    let title = resolved_string_field(input, "title").unwrap_or_default();
    let handle = resolved_string_field(input, "handle").unwrap_or_else(|| slugify_handle(&title));
    let body = resolved_string_field(input, "body").unwrap_or_default();
    let (is_published, published_at) = publication_state(input, existing, true);
    json!({
        "__typename": "Page",
        "id": id,
        "title": title,
        "handle": handle,
        "body": body,
        "bodySummary": body_summary(&body),
        "isPublished": is_published,
        "publishedAt": published_at,
        "createdAt": ONLINE_STORE_CONTENT_TIMESTAMP,
        "updatedAt": ONLINE_STORE_CONTENT_TIMESTAMP,
        "templateSuffix": optional_string_value(input, "templateSuffix")
    })
}

fn apply_page_input(record: &mut Value, input: &BTreeMap<String, ResolvedValue>) {
    if let Some(title) = resolved_string_field(input, "title") {
        record["title"] = json!(title);
        if !input.contains_key("handle") {
            record["handle"] = json!(slugify_handle(record["title"].as_str().unwrap_or_default()));
        }
    }
    if let Some(handle) = resolved_string_field(input, "handle") {
        record["handle"] = json!(handle);
    }
    if let Some(body) = resolved_string_field(input, "body") {
        record["body"] = json!(body);
        record["bodySummary"] = json!(body_summary(record["body"].as_str().unwrap_or_default()));
    }
    if input.contains_key("isPublished")
        || input.contains_key("visible")
        || input.contains_key("publishDate")
        || input.contains_key("visibilityDate")
    {
        let (is_published, published_at) = publication_state(input, Some(record), false);
        record["isPublished"] = json!(is_published);
        record["publishedAt"] = published_at;
    }
    if input.contains_key("templateSuffix") {
        record["templateSuffix"] = optional_string_value(input, "templateSuffix");
    }
    record["updatedAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
}

fn article_record(
    id: &str,
    blog_id: &str,
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
) -> Value {
    let title = resolved_string_field(input, "title").unwrap_or_default();
    let handle = resolved_string_field(input, "handle").unwrap_or_else(|| slugify_handle(&title));
    let body = resolved_string_field(input, "body").unwrap_or_default();
    let summary = optional_string_value(input, "summary");
    let (is_published, published_at) = publication_state(input, existing, true);
    json!({
        "__typename": "Article",
        "id": id,
        "blogId": blog_id,
        "title": title,
        "handle": handle,
        "body": body,
        "summary": summary,
        "tags": resolved_string_list_field(input, "tags"),
        "isPublished": is_published,
        "publishedAt": published_at,
        "createdAt": ONLINE_STORE_CONTENT_TIMESTAMP,
        "updatedAt": ONLINE_STORE_CONTENT_TIMESTAMP,
        "templateSuffix": optional_string_value(input, "templateSuffix"),
        "author": article_author_json(input),
        "image": article_image_json(input),
        "metafields": connection_json(Vec::new()),
        "commentsCount": publication_count_json(0),
        "comments": connection_json(Vec::new())
    })
}

fn apply_article_input(record: &mut Value, input: &BTreeMap<String, ResolvedValue>) {
    if let Some(title) = resolved_string_field(input, "title") {
        record["title"] = json!(title);
        if !input.contains_key("handle") {
            record["handle"] = json!(slugify_handle(record["title"].as_str().unwrap_or_default()));
        }
    }
    if let Some(handle) = resolved_string_field(input, "handle") {
        record["handle"] = json!(handle);
    }
    if let Some(body) = resolved_string_field(input, "body") {
        record["body"] = json!(body);
    }
    if input.contains_key("summary") {
        record["summary"] = optional_string_value(input, "summary");
    }
    if input.contains_key("tags") {
        record["tags"] = json!(resolved_string_list_field(input, "tags"));
    }
    if input.contains_key("author") || input.contains_key("authorV2") {
        record["author"] = article_author_json(input);
    }
    if input.contains_key("image") {
        record["image"] = article_image_json(input);
    }
    if input.contains_key("isPublished")
        || input.contains_key("visible")
        || input.contains_key("publishDate")
        || input.contains_key("visibilityDate")
    {
        let (is_published, published_at) = publication_state(input, Some(record), false);
        record["isPublished"] = json!(is_published);
        record["publishedAt"] = published_at;
    }
    if input.contains_key("templateSuffix") {
        record["templateSuffix"] = optional_string_value(input, "templateSuffix");
    }
    record["updatedAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
}

fn publication_state(
    input: &BTreeMap<String, ResolvedValue>,
    existing: Option<&Value>,
    create: bool,
) -> (bool, Value) {
    let supplied_published =
        resolved_bool_field(input, "isPublished").or_else(|| resolved_bool_field(input, "visible"));
    let supplied_date = resolved_string_field(input, "publishDate")
        .or_else(|| resolved_string_field(input, "visibilityDate"));
    let existing_published = existing
        .and_then(|record| record["isPublished"].as_bool())
        .unwrap_or(false);
    let existing_published_at = existing
        .map(|record| record["publishedAt"].clone())
        .unwrap_or(Value::Null);

    let is_published = supplied_published.unwrap_or_else(|| {
        if create && supplied_date.is_none() {
            true
        } else {
            existing_published
        }
    });
    let published_at = if let Some(date) = supplied_date {
        json!(date)
    } else if is_published {
        if existing_published_at.is_null() {
            json!(ONLINE_STORE_CONTENT_TIMESTAMP)
        } else {
            existing_published_at
        }
    } else {
        Value::Null
    };
    (is_published, published_at)
}

fn invalid_publish_date_error(
    input: &BTreeMap<String, ResolvedValue>,
    root: &'static str,
    effective_is_published: bool,
) -> Option<Value> {
    let publish_date = resolved_string_field(input, "publishDate")
        .or_else(|| resolved_string_field(input, "visibilityDate"));
    if effective_is_published && publish_date.as_deref().is_some_and(is_future_date) {
        Some(user_error(
            vec![root],
            "Can\u{2019}t set isPublished to true and also set a future publish date.",
            Some("INVALID_PUBLISH_DATE"),
        ))
    } else {
        None
    }
}

fn is_future_date(value: &str) -> bool {
    let Some(publish_date) = parse_rfc3339_epoch_seconds(value) else {
        return false;
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    publish_date > now.as_secs() as i64
}

fn title_blank_error(
    input: &BTreeMap<String, ResolvedValue>,
    root: &'static str,
    code: Option<&'static str>,
    required: bool,
) -> Option<Value> {
    let error = || match code {
        Some("BLANK") => presence_user_error(vec![root, "title"], "Title"),
        _ => user_error(vec![root, "title"], "Title can't be blank", code),
    };
    match input.get("title") {
        Some(ResolvedValue::String(title)) if title.trim().is_empty() => Some(error()),
        None if required => Some(error()),
        _ => None,
    }
}

fn content_length_error(
    input: &BTreeMap<String, ResolvedValue>,
    root: &'static str,
    handle_limit: usize,
    body_limit: Option<(usize, &'static str, Option<&'static str>)>,
) -> Option<Value> {
    if resolved_string_field(input, "title")
        .as_deref()
        .is_some_and(|title| title.chars().count() > ONLINE_STORE_TITLE_MAX_CHARS)
    {
        return Some(length_user_error(
            vec![root, "title"],
            "Title",
            LengthUserErrorBound::TooLong {
                maximum: ONLINE_STORE_TITLE_MAX_CHARS,
            },
        ));
    }
    if resolved_string_field(input, "handle")
        .as_deref()
        .is_some_and(|handle| handle.chars().count() > handle_limit)
    {
        return Some(length_user_error(
            vec![root, "handle"],
            "Handle",
            LengthUserErrorBound::TooLong {
                maximum: handle_limit,
            },
        ));
    }
    if let Some((limit, message, code)) = body_limit {
        if resolved_string_field(input, "body")
            .as_deref()
            .is_some_and(|body| body.len() > limit)
        {
            return Some(user_error(vec![root, "body"], message, code));
        }
    }
    None
}

fn commentable_inclusion_error(input: &BTreeMap<String, ResolvedValue>) -> Option<Value> {
    let commentable = resolved_string_field(input, "commentable")?;
    if matches!(
        commentable.as_str(),
        "NO" | "CLOSED" | "YES" | "MODERATE" | "MODERATED"
    ) {
        None
    } else {
        Some(user_error(
            vec!["blog", "commentable"],
            "Commentable is not included in the list",
            Some("INCLUSION"),
        ))
    }
}

fn article_author_error(
    input: &BTreeMap<String, ResolvedValue>,
    create: bool,
    required: bool,
) -> Option<Value> {
    let author = resolved_object_field(input, "author");
    let author_v2 = resolved_object_field(input, "authorV2");
    if author.is_none() && author_v2.is_none() && !required {
        return None;
    }
    let name = author
        .as_ref()
        .and_then(|author| resolved_string_field(author, "name"))
        .or_else(|| {
            author_v2
                .as_ref()
                .and_then(|author| resolved_string_field(author, "name"))
        });
    let user_id = author
        .as_ref()
        .and_then(|author| resolved_string_field(author, "userId"))
        .or_else(|| {
            author_v2
                .as_ref()
                .and_then(|author| resolved_string_field(author, "userId"))
        });
    let has_name = name.as_deref().is_some_and(|name| !name.trim().is_empty());
    let has_user_id = user_id
        .as_deref()
        .is_some_and(|user_id| !user_id.trim().is_empty());
    if has_name && has_user_id {
        return Some(user_error(
            vec!["article"],
            if create {
                "Can't create an article author if both author name and user ID are supplied."
            } else {
                "Can't update an article author if both author name and user ID are supplied."
            },
            Some("AMBIGUOUS_AUTHOR"),
        ));
    }
    if has_user_id {
        return Some(user_error(
            vec!["article"],
            "User must exist if a user ID is supplied.",
            Some("AUTHOR_MUST_EXIST"),
        ));
    }
    if (required || author.is_some() || author_v2.is_some()) && !has_name {
        return Some(user_error(
            vec!["article"],
            if create {
                "Can't create an article if both author name and user ID are blank."
            } else {
                "Can't update an article if both author name and user ID are blank."
            },
            Some("AUTHOR_FIELD_REQUIRED"),
        ));
    }
    None
}

fn article_image_update_error(
    record: &Value,
    input: &BTreeMap<String, ResolvedValue>,
) -> Option<Value> {
    let image = resolved_object_field(input, "image")?;
    let has_alt_text = resolved_string_field(&image, "altText")
        .as_deref()
        .is_some_and(|alt_text| !alt_text.trim().is_empty());
    let has_new_url = resolved_string_field(&image, "url")
        .as_deref()
        .is_some_and(|url| !url.trim().is_empty());
    let has_existing_url = record["image"]["url"]
        .as_str()
        .is_some_and(|url| !url.trim().is_empty());
    if has_alt_text && !has_new_url && !has_existing_url {
        Some(user_error(
            vec!["article", "image"],
            "Cannot update image alt text without an existing image or providing a new image URL",
            Some("INVALID"),
        ))
    } else {
        None
    }
}

fn article_author_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    if let Some(author) = resolved_object_field(input, "author") {
        if let Some(name) = resolved_string_field(&author, "name") {
            return json!({ "name": name });
        }
    }
    if let Some(author) = resolved_object_field(input, "authorV2") {
        if let Some(name) = resolved_string_field(&author, "name") {
            return json!({ "name": name });
        }
        if let Some(user_id) = resolved_string_field(&author, "userId") {
            return json!({ "name": user_id });
        }
    }
    Value::Null
}

fn article_image_json(input: &BTreeMap<String, ResolvedValue>) -> Value {
    let Some(image) = resolved_object_field(input, "image") else {
        return Value::Null;
    };
    let url = resolved_string_field(&image, "url");
    let alt_text = resolved_string_field(&image, "altText");
    if url.is_none() && alt_text.is_none() {
        return Value::Null;
    }
    json!({
        "url": url,
        "altText": alt_text
    })
}

fn body_summary(body: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for character in body.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn optional_string_value(input: &BTreeMap<String, ResolvedValue>, field: &str) -> Value {
    match input.get(field) {
        Some(ResolvedValue::String(value)) => json!(value),
        Some(ResolvedValue::Null) => Value::Null,
        _ => Value::Null,
    }
}

fn online_store_payload(
    selection: &[SelectedField],
    resource_key: &str,
    resource: Value,
    user_errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({
            resource_key: resource,
            "userErrors": user_errors
        }),
        selection,
    )
}

fn delete_payload(
    selection: &[SelectedField],
    deleted_key: &str,
    deleted_id: Value,
    user_errors: Vec<Value>,
) -> Value {
    selected_json(
        &json!({
            deleted_key: deleted_id,
            "userErrors": user_errors
        }),
        selection,
    )
}

fn article_blog_not_found_payload(selection: &[SelectedField], key: &str) -> Value {
    online_store_payload(
        selection,
        key,
        Value::Null,
        vec![user_error(
            vec!["article"],
            "Must reference an existing blog.",
            Some("NOT_FOUND"),
        )],
    )
}

fn comment_not_found_error() -> Value {
    user_error(vec!["id"], "Comment does not exist", Some("NOT_FOUND"))
}

fn online_store_count_with_baseline(
    baseline: Option<usize>,
    order: &[String],
    deleted_ids: &BTreeSet<String>,
) -> Option<usize> {
    let baseline = baseline?;
    let synthetic_staged = order
        .iter()
        .filter(|id| is_synthetic_gid(id))
        .filter(|id| !deleted_ids.contains(*id))
        .count();
    let deleted_baseline = deleted_ids
        .iter()
        .filter(|id| !is_synthetic_gid(id))
        .count();
    Some(baseline.saturating_sub(deleted_baseline) + synthetic_staged)
}

fn is_online_store_content_query_root(root: &str) -> bool {
    matches!(
        root,
        "article"
            | "articleAuthors"
            | "articles"
            | "articleTags"
            | "blog"
            | "blogs"
            | "blogsCount"
            | "page"
            | "pages"
            | "pagesCount"
            | "comment"
            | "comments"
    )
}

fn article_matches_query(article: &Value, query: &str) -> bool {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return true;
    }
    for key in ["title", "handle", "body", "summary"] {
        if article[key]
            .as_str()
            .is_some_and(|value| value.to_ascii_lowercase().contains(&query))
        {
            return true;
        }
    }
    article["tags"].as_array().is_some_and(|tags| {
        tags.iter().any(|tag| {
            tag.as_str()
                .is_some_and(|tag| tag.to_ascii_lowercase().contains(&query))
        })
    })
}

fn should_stage_observed_blog(record: &Value) -> bool {
    record.get("title").is_some()
        || record.get("handle").is_some()
        || record.get("commentPolicy").is_some()
        || record.get("articles").is_some()
}

fn should_stage_observed_page(record: &Value) -> bool {
    record.get("title").is_some() || record.get("handle").is_some() || record.get("body").is_some()
}

fn should_stage_observed_article(record: &Value) -> bool {
    record.get("title").is_some()
        || record.get("handle").is_some()
        || record.get("body").is_some()
        || record.get("comments").is_some()
}

fn should_stage_observed_comment(record: &Value) -> bool {
    record.get("status").is_some()
        || record.get("body").is_some()
        || record.get("bodyHtml").is_some()
        || record.get("article").is_some()
}

fn string_value(record: &Value, key: &str) -> Option<String> {
    record.get(key).and_then(Value::as_str).map(str::to_string)
}

fn bool_value(record: &Value, key: &str) -> Option<bool> {
    record.get(key).and_then(Value::as_bool)
}

fn normalize_observed_blog(record: &Value) -> Value {
    let mut record = record.clone();
    let title = string_value(&record, "title").unwrap_or_default();
    let handle = string_value(&record, "handle").unwrap_or_else(|| slugify_handle(&title));
    let articles = connection_nodes(&record["articles"]);
    record["__typename"] = json!("Blog");
    record["title"] = json!(title);
    record["handle"] = json!(handle);
    if record.get("commentPolicy").is_none() {
        record["commentPolicy"] = json!("CLOSED");
    }
    if record.get("tags").is_none() {
        record["tags"] = json!([]);
    }
    if record.get("templateSuffix").is_none() {
        record["templateSuffix"] = Value::Null;
    }
    if record.get("createdAt").is_none() {
        record["createdAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
    }
    if record.get("updatedAt").is_none() {
        record["updatedAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
    }
    if record.get("articlesCount").is_none() {
        record["articlesCount"] = publication_count_json(articles.len());
    }
    if record.get("articles").is_none() {
        record["articles"] = connection_json(Vec::new());
    }
    record
}

fn normalize_observed_page(record: &Value) -> Value {
    let mut record = record.clone();
    let title = string_value(&record, "title").unwrap_or_default();
    let handle = string_value(&record, "handle").unwrap_or_else(|| slugify_handle(&title));
    let body = string_value(&record, "body").unwrap_or_default();
    record["__typename"] = json!("Page");
    record["title"] = json!(title);
    record["handle"] = json!(handle);
    record["body"] = json!(body);
    if record.get("bodySummary").is_none() {
        record["bodySummary"] = json!(body_summary(record["body"].as_str().unwrap_or_default()));
    }
    if record.get("isPublished").is_none() {
        record["isPublished"] = json!(false);
    }
    if record.get("publishedAt").is_none() {
        record["publishedAt"] = Value::Null;
    }
    if record.get("createdAt").is_none() {
        record["createdAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
    }
    if record.get("updatedAt").is_none() {
        record["updatedAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
    }
    if record.get("templateSuffix").is_none() {
        record["templateSuffix"] = Value::Null;
    }
    record
}

fn normalize_observed_article(record: &Value, parent_blog_id: Option<&str>) -> Value {
    let mut record = record.clone();
    let title = string_value(&record, "title").unwrap_or_default();
    let handle = string_value(&record, "handle").unwrap_or_else(|| slugify_handle(&title));
    let body = string_value(&record, "body").unwrap_or_default();
    let blog_id = string_value(&record, "blogId")
        .or_else(|| {
            record
                .get("blog")
                .and_then(|blog| blog.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| parent_blog_id.map(str::to_string))
        .unwrap_or_default();
    let comments = connection_nodes(&record["comments"]);
    record["__typename"] = json!("Article");
    record["blogId"] = json!(blog_id);
    record["title"] = json!(title);
    record["handle"] = json!(handle);
    record["body"] = json!(body);
    if record.get("summary").is_none() {
        record["summary"] = Value::Null;
    }
    if record.get("tags").is_none() {
        record["tags"] = json!([]);
    }
    if record.get("isPublished").is_none() {
        record["isPublished"] = json!(false);
    }
    if record.get("publishedAt").is_none() {
        record["publishedAt"] = Value::Null;
    }
    if record.get("createdAt").is_none() {
        record["createdAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
    }
    if record.get("updatedAt").is_none() {
        record["updatedAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
    }
    if record.get("templateSuffix").is_none() {
        record["templateSuffix"] = Value::Null;
    }
    if record.get("author").is_none() {
        record["author"] = Value::Null;
    }
    if record.get("image").is_none() {
        record["image"] = Value::Null;
    }
    if record.get("metafields").is_none() {
        record["metafields"] = connection_json(Vec::new());
    }
    if record.get("commentsCount").is_none() {
        record["commentsCount"] = publication_count_json(comments.len());
    }
    if record.get("comments").is_none() {
        record["comments"] = connection_json(Vec::new());
    }
    record
}

fn normalize_observed_comment(record: &Value, parent_article_id: Option<&str>) -> Value {
    let mut record = record.clone();
    let status = string_value(&record, "status")
        .map(|status| match status.as_str() {
            "pending" => "UNAPPROVED".to_string(),
            "published" => "PUBLISHED".to_string(),
            "spam" => "SPAM".to_string(),
            _ => status,
        })
        .unwrap_or_else(|| "UNAPPROVED".to_string());
    let article_id = string_value(&record, "articleId")
        .or_else(|| {
            record
                .get("article")
                .and_then(|article| article.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| parent_article_id.map(str::to_string))
        .unwrap_or_default();
    let body = string_value(&record, "body")
        .or_else(|| string_value(&record, "bodyHtml"))
        .unwrap_or_default();
    let body_html = string_value(&record, "bodyHtml").unwrap_or_else(|| body.clone());
    let is_published = bool_value(&record, "isPublished").unwrap_or(status == "PUBLISHED");
    record["__typename"] = json!("Comment");
    record["articleId"] = json!(article_id);
    record["status"] = json!(status);
    record["isPublished"] = json!(is_published);
    record["body"] = json!(body);
    record["bodyHtml"] = json!(body_html);
    if record.get("publishedAt").is_none() {
        record["publishedAt"] = Value::Null;
    }
    if record.get("createdAt").is_none() {
        record["createdAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
    }
    if record.get("updatedAt").is_none() {
        record["updatedAt"] = json!(ONLINE_STORE_CONTENT_TIMESTAMP);
    }
    record
}
