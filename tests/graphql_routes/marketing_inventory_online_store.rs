use super::common::*;
use pretty_assertions::assert_eq;

fn assert_core_metaobject_auto_handle(handle: &str, prefix: &str) {
    let suffix = handle
        .strip_prefix(prefix)
        .unwrap_or_else(|| panic!("expected handle {handle:?} to start with {prefix:?}"));
    assert_eq!(
        suffix.len(),
        8,
        "auto handle suffix should be eight characters: {handle:?}"
    );
    assert!(
        suffix
            .chars()
            .all(|character| character.is_ascii_lowercase() || character.is_ascii_digit()),
        "auto handle suffix should be lowercase alphanumeric: {handle:?}"
    );
}

fn assert_online_store_operation_timestamp(value: &Value, context: &str) -> String {
    let timestamp = value
        .as_str()
        .unwrap_or_else(|| panic!("{context} should be a timestamp string"));
    time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|error| panic!("{context} should parse as RFC3339: {error}"));
    assert_ne!(timestamp, "2024-01-01T00:00:00.000Z", "{context}");
    assert_ne!(timestamp, "2024-01-01T00:00:01.000Z", "{context}");
    timestamp.to_string()
}

fn create_metaobject_definition_for_test(
    proxy: &mut DraftProxy,
    meta_type: &str,
    field_definitions: Vec<Value>,
) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDefinitionForTest($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"definition": {
            "type": meta_type,
            "name": meta_type,
            "displayNameKey": "title",
            "fieldDefinitions": field_definitions
        }}),
    ));
    assert_eq!(
        response.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn metaobject_url_redirects_stage_and_read_after_definition_url_handle_update() {
    let mut proxy = snapshot_proxy();
    let definition_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateRedirectDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition {
              id
              capabilities { onlineStore { data { urlHandle canCreateRedirects } } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"definition": {
            "type": "redirect_definition_test",
            "name": "Redirect definition test",
            "displayNameKey": "title",
            "access": {"storefront": "PUBLIC_READ"},
            "capabilities": {
                "publishable": {"enabled": true},
                "renderable": {"enabled": true, "data": {"metaTitleKey": "title"}},
                "onlineStore": {"enabled": true, "data": {"urlHandle": "old-redirect-definition"}}
            },
            "fieldDefinitions": [
                {"key": "title", "name": "Title", "type": "single_line_text_field", "required": true}
            ]
        }}),
    ));
    assert_eq!(
        definition_create.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        definition_create.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"]
            ["capabilities"]["onlineStore"]["data"],
        json!({"urlHandle": "old-redirect-definition", "canCreateRedirects": true})
    );
    let definition_id = definition_create.body["data"]["metaobjectDefinitionCreate"]
        ["metaobjectDefinition"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let create_entry = r#"
        mutation CreateRedirectEntry($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id handle }
            userErrors { field message code }
          }
        }
    "#;
    for (handle, title) in [
        ("first-redirect-row", "First redirect row"),
        ("second-redirect-row", "Second redirect row"),
    ] {
        let entry = proxy.process_request(json_graphql_request(
            create_entry,
            json!({"metaobject": {
                "type": "redirect_definition_test",
                "handle": handle,
                "capabilities": {
                    "publishable": {"status": "ACTIVE"},
                    "onlineStore": {"templateSuffix": ""}
                },
                "fields": [{"key": "title", "value": title}]
            }}),
        ));
        assert_eq!(
            entry.body["data"]["metaobjectCreate"]["userErrors"],
            json!([])
        );
    }

    let definition_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateRedirectDefinition($id: ID!, $definition: MetaobjectDefinitionUpdateInput!) {
          metaobjectDefinitionUpdate(id: $id, definition: $definition) {
            metaobjectDefinition {
              capabilities { onlineStore { data { urlHandle canCreateRedirects } } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": definition_id,
            "definition": {
                "capabilities": {
                    "onlineStore": {
                        "enabled": true,
                        "data": {"urlHandle": "new-redirect-definition", "createRedirects": true}
                    }
                }
            }
        }),
    ));
    assert_eq!(
        definition_update.body["data"]["metaobjectDefinitionUpdate"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadDefinitionRedirects($pathQuery: String!, $targetQuery: String!) {
          byPath: urlRedirects(first: 1, query: $pathQuery) {
            nodes { id path target }
          }
          reverseByTarget: urlRedirects(first: 1, sortKey: TARGET, reverse: true) {
            nodes { path target }
          }
          matchingTargetCount: urlRedirectsCount(query: $targetQuery) { count precision }
        }
        "#,
        json!({
            "pathQuery": "path:/pages/old-redirect-definition/first-redirect-row",
            "targetQuery": "target:/pages/new-redirect-definition/first-redirect-row"
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["byPath"]["nodes"][0]["path"],
        json!("/pages/old-redirect-definition/first-redirect-row")
    );
    assert_eq!(
        read.body["data"]["byPath"]["nodes"][0]["target"],
        json!("/pages/new-redirect-definition/first-redirect-row")
    );
    assert_eq!(
        read.body["data"]["reverseByTarget"]["nodes"][0]["path"],
        json!("/pages/old-redirect-definition/second-redirect-row")
    );
    assert_eq!(
        read.body["data"]["matchingTargetCount"],
        json!({"count": 1, "precision": "EXACT"})
    );

    let redirect_id = read.body["data"]["byPath"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let singular = proxy.process_request(json_graphql_request(
        r#"
        query ReadDefinitionRedirect($id: ID!) {
          urlRedirect(id: $id) { path target }
        }
        "#,
        json!({"id": redirect_id}),
    ));
    assert_eq!(
        singular.body["data"]["urlRedirect"],
        json!({
            "path": "/pages/old-redirect-definition/first-redirect-row",
            "target": "/pages/new-redirect-definition/first-redirect-row"
        })
    );

    let handle_entry = proxy.process_request(json_graphql_request(
        create_entry,
        json!({"metaobject": {
            "type": "redirect_definition_test",
            "handle": "handle-redirect-old",
            "capabilities": {
                "publishable": {"status": "ACTIVE"},
                "onlineStore": {"templateSuffix": ""}
            },
            "fields": [{"key": "title", "value": "Handle redirect row"}]
        }}),
    ));
    let handle_entry_id = handle_entry.body["data"]["metaobjectCreate"]["metaobject"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let handle_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateRedirectHandle($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject { handle }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": handle_entry_id,
            "metaobject": {
                "handle": "handle-redirect-new",
                "redirectNewHandle": true,
                "fields": [{"key": "title", "value": "Handle redirect row"}]
            }
        }),
    ));
    assert_eq!(
        handle_update.body["data"]["metaobjectUpdate"]["userErrors"],
        json!([])
    );
    let handle_redirect = proxy.process_request(json_graphql_request(
        r#"
        query ReadHandleRedirect($query: String!) {
          urlRedirects(first: 1, query: $query) { nodes { path target } }
        }
        "#,
        json!({"query": "path:/pages/new-redirect-definition/handle-redirect-old"}),
    ));
    assert_eq!(
        handle_redirect.body["data"]["urlRedirects"]["nodes"][0],
        json!({
            "path": "/pages/new-redirect-definition/handle-redirect-old",
            "target": "/pages/new-redirect-definition/handle-redirect-new"
        })
    );

    let dump = proxy.process_request(request_with_body(
        "POST",
        "/__meta/dump",
        &json!({ "createdAt": "2026-07-04T20:40:00.000Z" }).to_string(),
    ));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["state"]["stagedState"]["urlRedirects"]
            .as_object()
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        dump.body["state"]["stagedState"]["urlRedirectOrder"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    let mut restored = snapshot_proxy();
    let restore = restored.process_request(request_with_body(
        "POST",
        "/__meta/restore",
        &dump.body.to_string(),
    ));
    assert_eq!(restore.status, 200);
    let restored_read = restored.process_request(json_graphql_request(
        r#"
        query RestoredRedirects($query: String!) {
          urlRedirects(first: 1, query: $query) { nodes { path target } }
        }
        "#,
        json!({"query": "target:/pages/new-redirect-definition/handle-redirect-new"}),
    ));
    assert_eq!(
        restored_read.body["data"]["urlRedirects"]["nodes"][0],
        json!({
            "path": "/pages/new-redirect-definition/handle-redirect-old",
            "target": "/pages/new-redirect-definition/handle-redirect-new"
        })
    );
}

#[test]
fn metaobject_definition_list_scalar_field_categories_follow_element_type() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateListScalarCategoryDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition {
              fieldDefinitions { key type { name category } }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"definition": {
            "type": "list_scalar_category",
            "name": "List scalar category",
            "fieldDefinitions": [
                {"key": "text_values", "name": "Text values", "type": "list.single_line_text_field", "required": false},
                {"key": "numbers", "name": "Numbers", "type": "list.number_integer", "required": false},
                {"key": "dates", "name": "Dates", "type": "list.date", "required": false},
                {"key": "references", "name": "References", "type": "list.metaobject_reference", "required": false}
            ]
        }}),
    ));

    assert_eq!(
        response.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"]
            ["fieldDefinitions"],
        json!([
            {"key": "text_values", "type": {"name": "list.single_line_text_field", "category": "TEXT"}},
            {"key": "numbers", "type": {"name": "list.number_integer", "category": "NUMBER"}},
            {"key": "dates", "type": {"name": "list.date", "category": "DATE_TIME"}},
            {"key": "references", "type": {"name": "list.metaobject_reference", "category": "REFERENCE"}}
        ])
    );
}

#[test]
fn metaobject_definition_has_thumbnail_field_tracks_file_reference_on_create_and_update() {
    let mut proxy = snapshot_proxy();
    let create_definition = r#"
        mutation CreateThumbnailDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id hasThumbnailField fieldDefinitions { key type { name } } }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;

    let created_with_thumbnail = proxy.process_request(json_graphql_request(
        create_definition,
        json!({"definition": {
            "type": "thumbnail_create_definition",
            "name": "Thumbnail create definition",
            "displayNameKey": "title",
            "fieldDefinitions": [
                {"key": "title", "name": "Title", "type": "single_line_text_field", "required": false},
                {"key": "hero_image", "name": "Hero image", "type": "file_reference", "required": false}
            ]
        }}),
    ));
    assert_eq!(
        created_with_thumbnail.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        created_with_thumbnail.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"]
            ["hasThumbnailField"],
        json!(true)
    );

    let created_without_thumbnail = proxy.process_request(json_graphql_request(
        create_definition,
        json!({"definition": {
            "type": "thumbnail_update_definition",
            "name": "Thumbnail update definition",
            "displayNameKey": "title",
            "fieldDefinitions": [
                {"key": "title", "name": "Title", "type": "single_line_text_field", "required": false}
            ]
        }}),
    ));
    assert_eq!(
        created_without_thumbnail.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        created_without_thumbnail.body["data"]["metaobjectDefinitionCreate"]
            ["metaobjectDefinition"]["hasThumbnailField"],
        json!(false)
    );
    let definition_id = created_without_thumbnail.body["data"]["metaobjectDefinitionCreate"]
        ["metaobjectDefinition"]["id"]
        .as_str()
        .unwrap();

    let updated = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateThumbnailDefinition($id: ID!, $definition: MetaobjectDefinitionUpdateInput!) {
          metaobjectDefinitionUpdate(id: $id, definition: $definition) {
            metaobjectDefinition { hasThumbnailField fieldDefinitions { key type { name } } }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"id": definition_id, "definition": {
            "fieldDefinitions": [{
                "create": {"key": "thumbnail", "name": "Thumbnail", "type": "file_reference", "required": false}
            }]
        }}),
    ));
    assert_eq!(
        updated.body["data"]["metaobjectDefinitionUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        updated.body["data"]["metaobjectDefinitionUpdate"]["metaobjectDefinition"]
            ["hasThumbnailField"],
        json!(true)
    );
}

#[test]
fn metaobject_definition_create_accepts_current_custom_data_field_types() {
    let mut proxy = snapshot_proxy();
    let create_definition = r#"
        mutation CreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition {
              fieldDefinitions { key type { name } }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;

    for field_type in [
        "jurisdiction",
        "list.jurisdiction",
        "product_taxonomy_disclosure_reference",
    ] {
        let key = field_type.replace('.', "_");
        let response = proxy.process_request(json_graphql_request(
            create_definition,
            json!({"definition": {
                "type": format!("accepted_{}", key),
                "name": format!("Accepted {field_type}"),
                "displayNameKey": key,
                "fieldDefinitions": [{
                    "key": key,
                    "name": "Field",
                    "type": field_type,
                    "required": false
                }]
            }}),
        ));
        let payload = &response.body["data"]["metaobjectDefinitionCreate"];
        assert_eq!(payload["userErrors"], json!([]), "{field_type}");
        assert_eq!(
            payload["metaobjectDefinition"]["fieldDefinitions"][0]["type"]["name"],
            json!(field_type)
        );
    }

    for field_type in ["disclosure_reference", "list.disclosure_reference"] {
        let key = field_type.replace('.', "_");
        let response = proxy.process_request(json_graphql_request(
            create_definition,
            json!({"definition": {
                "type": format!("standard_only_{}", key),
                "name": format!("Standard-only {field_type}"),
                "displayNameKey": key,
                "fieldDefinitions": [{
                    "key": key,
                    "name": "Field",
                    "type": field_type,
                    "required": false
                }]
            }}),
        ));
        assert_eq!(
            response.body["data"]["metaobjectDefinitionCreate"],
            json!({
                "metaobjectDefinition": null,
                "userErrors": [{
                    "field": ["definition", "fieldDefinitions", "0"],
                    "message": "The disclosure_reference type can only be used in standard definitions provided by Shopify.",
                    "code": "INVALID",
                    "elementKey": key,
                    "elementIndex": null
                }]
            })
        );
    }

    let unknown_type = proxy.process_request(json_graphql_request(
        create_definition,
        json!({"definition": {
            "type": "rejected_unknown_field_type",
            "name": "Rejected unknown field type",
            "displayNameKey": "title",
            "fieldDefinitions": [{
                "key": "title",
                "name": "Title",
                "type": "garbage_type",
                "required": false
            }]
        }}),
    ));
    let message = unknown_type.body["data"]["metaobjectDefinitionCreate"]["userErrors"][0]
        ["message"]
        .as_str()
        .expect("unknown type message");
    assert!(message.contains("jurisdiction"));
    assert!(message.contains("list.jurisdiction"));
    assert!(message.contains("product_taxonomy_disclosure_reference"));
    assert!(message.contains("disclosure_reference"));
    assert!(message.contains("list.disclosure_reference"));
}

#[test]
fn marketing_empty_reads_keep_shopify_connection_shapes() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        query MarketingBaselineRead($activityId: ID!, $eventId: ID!, $first: Int!, $activityQuery: String!, $eventQuery: String!) {
          marketingActivities(first: $first, sortKey: CREATED_AT, reverse: true) { nodes { id title } edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          marketingActivitiesEmpty: marketingActivities(first: $first, query: $activityQuery, sortKey: TITLE) { nodes { id title } edges { cursor node { id title } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          marketingActivity(id: $activityId) { id title }
          marketingEvents(first: $first) { nodes { id type } edges { cursor node { id type } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          marketingEventsEmpty: marketingEvents(first: $first, query: $eventQuery) { nodes { id type } edges { cursor node { id type } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          marketingEvent(id: $eventId) { id type }
        }
        "#,
        json!({
            "activityId": "gid://shopify/MarketingActivity/999999999999",
            "eventId": "gid://shopify/MarketingEvent/999999999999",
            "first": 3,
            "activityQuery": "title:__none__",
            "eventQuery": "description:__none__"
        }),
    ));

    assert_eq!(response.body["data"]["marketingActivity"], Value::Null);
    assert_eq!(response.body["data"]["marketingEvent"], Value::Null);
    assert_eq!(
        response.body["data"]["marketingActivities"]["nodes"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["marketingActivities"]["edges"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["marketingActivities"]["pageInfo"],
        json!({"hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null})
    );
}

#[test]
fn marketing_activity_connections_honor_sort_window_and_query_for_staged_records() {
    let mut proxy = snapshot_proxy();
    let mut create_alpha_request = json_graphql_request(
        r#"
        mutation SeedMarketingActivity($input: MarketingActivityCreateExternalInput!) {
          created: marketingActivityCreateExternal(input: $input) {
            marketingActivity { id title createdAt app { title } marketingEvent { id type } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {
                "title": "Alpha launch",
                "remoteId": "alpha-launch",
                "status": "ACTIVE",
                "remoteUrl": "https://example.com/alpha",
                "tactic": "NEWSLETTER",
                "marketingChannelType": "EMAIL",
                "scheduledEnd": "2024-01-03T00:00:00.000Z",
                "utm": {"campaign": "alpha", "source": "email", "medium": "newsletter"}
        }}),
    );
    create_alpha_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "gid://shopify/App/42".to_string(),
    );
    let create_alpha = proxy.process_request(create_alpha_request);
    let mut create_zulu_request = json_graphql_request(
        r#"
        mutation SeedMarketingActivity($input: MarketingActivityCreateExternalInput!) {
          created: marketingActivityCreateExternal(input: $input) {
            marketingActivity { id title createdAt app { title } marketingEvent { id type } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {
                "title": "Zulu launch",
                "remoteId": "zulu-launch",
                "status": "ACTIVE",
                "remoteUrl": "https://example.com/zulu",
                "tactic": "AD",
                "marketingChannelType": "SEARCH",
                "scheduledEnd": "2024-01-04T00:00:00.000Z",
                "utm": {"campaign": "zulu", "source": "search", "medium": "ad"}
        }}),
    );
    create_zulu_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "gid://shopify/App/42".to_string(),
    );
    let create_zulu = proxy.process_request(create_zulu_request);
    assert_eq!(
        create_alpha.body["data"]["created"]["userErrors"],
        json!([])
    );
    assert_eq!(create_zulu.body["data"]["created"]["userErrors"], json!([]));
    assert_eq!(
        create_alpha.body["data"]["created"]["marketingActivity"]["app"],
        json!({ "title": "shopify-draft-proxy" })
    );
    assert_eq!(
        create_zulu.body["data"]["created"]["marketingActivity"]["app"],
        json!({ "title": "shopify-draft-proxy" })
    );

    let alpha_id = create_alpha.body["data"]["created"]["marketingActivity"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let zulu_id = create_zulu.body["data"]["created"]["marketingActivity"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let alpha_event_id = create_alpha.body["data"]["created"]["marketingActivity"]
        ["marketingEvent"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let zulu_event_id = create_zulu.body["data"]["created"]["marketingActivity"]["marketingEvent"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MarketingActivityConnectionRead(
          $activityCursor: String!
          $activityQuery: String!
          $eventQuery: String!
          $titleQuery: String!
          $createdAtQuery: String!
          $idRangeQuery: String!
          $scheduledEndQuery: String!
          $appIdQuery: String!
          $appNameQuery: String!
          $unknownFieldQuery: String!
        ) {
          latestActivity: marketingActivities(first: 1, sortKey: CREATED_AT, reverse: true) {
            nodes { id title createdAt }
            edges { cursor node { id title } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          afterLatest: marketingActivities(first: 1, after: $activityCursor, sortKey: CREATED_AT, reverse: true) {
            nodes { id title }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          titleSearch: marketingActivities(first: 5, sortKey: TITLE, query: $activityQuery) {
            nodes { id title }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          titleFilter: marketingActivities(first: 5, sortKey: ID, query: $titleQuery) {
            nodes { id title }
          }
          createdAtFilter: marketingActivities(first: 5, sortKey: ID, query: $createdAtQuery) {
            nodes { id title }
          }
          idRangeFilter: marketingActivities(first: 5, sortKey: ID, query: $idRangeQuery) {
            nodes { id title }
          }
          scheduledEndFilter: marketingActivities(first: 5, sortKey: ID, query: $scheduledEndQuery) {
            nodes { id title }
          }
          appIdFilter: marketingActivities(first: 5, sortKey: ID, query: $appIdQuery) {
            nodes { id title }
          }
          appNameFilter: marketingActivities(first: 5, sortKey: ID, query: $appNameQuery) {
            nodes { id title }
          }
          unknownFieldFallback: marketingActivities(first: 5, sortKey: ID, query: $unknownFieldQuery) {
            nodes { id title }
          }
          latestEvent: marketingEvents(first: 1, sortKey: ID, reverse: true) {
            nodes { id type }
            edges { cursor node { id type } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          eventSearch: marketingEvents(first: 5, query: $eventQuery) {
            nodes { id type }
          }
        }
        "#,
        json!({
            "activityCursor": format!("cursor:{zulu_id}"),
            "activityQuery": "launch",
            "eventQuery": "tactic:newsletter",
            "titleQuery": "title:\"Zulu launch\"",
            "createdAtQuery": "created_at:>=2024-01-01T00:00:02.000Z",
            "idRangeQuery": "id:>1",
            "scheduledEndQuery": "scheduled_to_end_at:2024-01-04",
            "appIdQuery": "app_id:42",
            "appNameQuery": "app_name:shopify-draft-proxy",
            "unknownFieldQuery": "unknown_field:\"Alpha launch\""
        }),
    ));

    assert_eq!(
        read.body["data"]["latestActivity"]["nodes"],
        json!([{ "id": zulu_id, "title": "Zulu launch", "createdAt": "2024-01-01T00:00:02.000Z" }])
    );
    assert_eq!(
        read.body["data"]["latestActivity"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": format!("cursor:{zulu_id}"),
            "endCursor": format!("cursor:{zulu_id}")
        })
    );
    assert_eq!(
        read.body["data"]["afterLatest"]["nodes"],
        json!([{ "id": alpha_id, "title": "Alpha launch" }])
    );
    assert_eq!(
        read.body["data"]["afterLatest"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );
    assert_eq!(
        read.body["data"]["titleSearch"]["nodes"],
        json!([
            { "id": alpha_id, "title": "Alpha launch" },
            { "id": zulu_id, "title": "Zulu launch" }
        ])
    );
    assert_eq!(
        read.body["data"]["titleFilter"]["nodes"],
        json!([{ "id": zulu_id, "title": "Zulu launch" }])
    );
    assert_eq!(
        read.body["data"]["createdAtFilter"]["nodes"],
        json!([{ "id": zulu_id, "title": "Zulu launch" }])
    );
    assert_eq!(
        read.body["data"]["idRangeFilter"]["nodes"],
        json!([{ "id": zulu_id, "title": "Zulu launch" }])
    );
    assert_eq!(
        read.body["data"]["scheduledEndFilter"]["nodes"],
        json!([{ "id": zulu_id, "title": "Zulu launch" }])
    );
    assert_eq!(
        read.body["data"]["appIdFilter"]["nodes"],
        json!([
            { "id": alpha_id, "title": "Alpha launch" },
            { "id": zulu_id, "title": "Zulu launch" }
        ])
    );
    assert_eq!(
        read.body["data"]["appNameFilter"]["nodes"],
        json!([
            { "id": alpha_id, "title": "Alpha launch" },
            { "id": zulu_id, "title": "Zulu launch" }
        ])
    );
    assert_eq!(
        read.body["data"]["unknownFieldFallback"]["nodes"],
        json!([{ "id": alpha_id, "title": "Alpha launch" }])
    );
    assert_eq!(
        read.body["data"]["latestEvent"]["nodes"],
        json!([{ "id": zulu_event_id, "type": "AD" }])
    );
    assert_eq!(
        read.body["data"]["latestEvent"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(
        read.body["data"]["eventSearch"]["nodes"],
        json!([{ "id": alpha_event_id, "type": "NEWSLETTER" }])
    );
}

#[test]
fn marketing_activity_queries_treat_boolean_operators_as_logic() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation SeedMarketingActivity($input: MarketingActivityCreateExternalInput!) {
          created: marketingActivityCreateExternal(input: $input) {
            marketingActivity { id title status }
            userErrors { field message code }
          }
        }
    "#;
    let active = proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {
            "title": "Active newsletter",
            "remoteId": "active-newsletter",
            "status": "ACTIVE",
            "remoteUrl": "https://example.com/active",
            "tactic": "NEWSLETTER",
            "marketingChannelType": "EMAIL",
            "utm": {"campaign": "active", "source": "email", "medium": "newsletter"}
        }}),
    ));
    let paused = proxy.process_request(json_graphql_request(
        create_query,
        json!({"input": {
            "title": "Paused newsletter",
            "remoteId": "paused-newsletter",
            "status": "PAUSED",
            "remoteUrl": "https://example.com/paused",
            "tactic": "NEWSLETTER",
            "marketingChannelType": "EMAIL",
            "utm": {"campaign": "paused", "source": "email", "medium": "newsletter"}
        }}),
    ));
    assert_eq!(active.body["data"]["created"]["userErrors"], json!([]));
    assert_eq!(paused.body["data"]["created"]["userErrors"], json!([]));
    let active_id = active.body["data"]["created"]["marketingActivity"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let paused_id = paused.body["data"]["created"]["marketingActivity"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MarketingActivityBooleanSearch(
          $orQuery: String!
          $andQuery: String!
          $exclusiveAndQuery: String!
        ) {
          statusOr: marketingActivities(first: 5, sortKey: ID, query: $orQuery) {
            nodes { id title status }
          }
          statusAnd: marketingActivities(first: 5, sortKey: ID, query: $andQuery) {
            nodes { id title status }
          }
          exclusiveAnd: marketingActivities(first: 5, sortKey: ID, query: $exclusiveAndQuery) {
            nodes { id title status }
          }
        }
        "#,
        json!({
            "orQuery": "status:ACTIVE OR status:PAUSED",
            "andQuery": "status:ACTIVE AND title:\"Active newsletter\"",
            "exclusiveAndQuery": "status:ACTIVE AND status:PAUSED"
        }),
    ));

    assert_eq!(
        read.body["data"]["statusOr"]["nodes"],
        json!([
            { "id": active_id, "title": "Active newsletter", "status": "ACTIVE" },
            { "id": paused_id, "title": "Paused newsletter", "status": "PAUSED" }
        ])
    );
    assert_eq!(
        read.body["data"]["statusAnd"]["nodes"],
        json!([
            { "id": active_id, "title": "Active newsletter", "status": "ACTIVE" }
        ])
    );
    assert_eq!(read.body["data"]["exclusiveAnd"]["nodes"], json!([]));
}

#[test]
fn marketing_external_activity_lifecycle_stages_updates_engagements_and_reads_back() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycle($input: MarketingActivityCreateExternalInput!) {
          createExternal: marketingActivityCreateExternal(input: $input) {
            marketingActivity { id title status statusLabel remoteId sourceAndMedium utmParameters { campaign source medium } marketingEvent { id remoteId manageUrl previewUrl sourceAndMedium } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"title": "Launch", "remoteId": "remote-1", "status": "ACTIVE", "remoteUrl": "https://example.com/manage", "previewUrl": "https://example.com/preview", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "launch", "source": "email", "medium": "newsletter"}}}),
    ));
    let activity_id = create.body["data"]["createExternal"]["marketingActivity"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"]["createExternal"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["createExternal"]["marketingActivity"]["title"],
        json!("Launch")
    );
    assert_eq!(
        create.body["data"]["createExternal"]["marketingActivity"]["statusLabel"],
        json!("Sending")
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycleUpdate($remoteId: String!, $utm: UTMInput, $input: MarketingActivityUpdateExternalInput!) {
          updateExternalByRemoteId: marketingActivityUpdateExternal(remoteId: $remoteId, utm: $utm, input: $input) {
            marketingActivity { id title status statusLabel marketingEvent { remoteId manageUrl description } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"remoteId": "remote-1", "utm": {"campaign": "launch", "source": "email", "medium": "newsletter"}, "input": {"title": "Launch updated", "status": "PAUSED", "remoteUrl": "https://example.com/manage-2"}}),
    ));
    assert_eq!(
        update.body["data"]["updateExternalByRemoteId"]["marketingActivity"]["title"],
        json!("Launch updated")
    );
    assert_eq!(
        update.body["data"]["updateExternalByRemoteId"]["marketingActivity"]["statusLabel"],
        json!("Paused")
    );

    let engagement = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingEngagementLifecycle($remoteId: String!, $engagement: MarketingEngagementInput!) {
          createByRemoteId: marketingEngagementCreate(remoteId: $remoteId, marketingEngagement: $engagement) {
            marketingEngagement { occurredOn impressionsCount clicksCount adSpend { amount currencyCode } marketingActivity { adSpend { amount currencyCode } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"remoteId": "remote-1", "engagement": {"occurredOn": "2026-04-26", "impressionsCount": 7, "clicksCount": 2, "adSpend": {"amount": "3.21", "currencyCode": "USD"}, "isCumulative": false, "utcOffset": "+00:00"}}),
    ));
    assert_eq!(
        engagement.body["data"]["createByRemoteId"]["userErrors"],
        json!([])
    );
    assert_eq!(
        engagement.body["data"]["createByRemoteId"]["marketingEngagement"]["marketingActivity"]
            ["adSpend"],
        json!(null)
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MarketingActivityRead($id: ID!, $remoteIds: [String!]) {
          marketingActivity(id: $id) { id title status statusLabel adSpend { amount currencyCode } marketingEvent { remoteId manageUrl description } }
          marketingActivities(first: 10, remoteIds: $remoteIds) { nodes { title marketingEvent { remoteId } } }
        }
        "#,
        json!({"id": activity_id, "remoteIds": ["remote-1"]}),
    ));
    assert_eq!(
        read.body["data"]["marketingActivity"]["title"],
        json!("Launch updated")
    );
    assert_eq!(
        read.body["data"]["marketingActivity"]["adSpend"],
        json!(null)
    );
    assert_eq!(
        read.body["data"]["marketingActivities"]["nodes"][0]["marketingEvent"]["remoteId"],
        json!("remote-1")
    );
}

#[test]
fn marketing_money_defaults_use_shop_currency_when_currency_code_is_omitted() {
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "CAD");

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingMoneyDefaults($activityInput: MarketingActivityCreateExternalInput!) {
          activity: marketingActivityCreateExternal(input: $activityInput) {
            marketingActivity {
              budget { total { amount currencyCode } }
              adSpend { amount currencyCode }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "activityInput": {
                "title": "Currency defaults",
                "remoteId": "currency-defaults",
                "status": "ACTIVE",
                "remoteUrl": "https://example.com/currency-defaults",
                "tactic": "NEWSLETTER",
                "marketingChannelType": "EMAIL",
                "utm": {
                    "campaign": "currency-defaults",
                    "source": "email",
                    "medium": "newsletter"
                },
                "budget": {
                    "budgetType": "DAILY",
                    "total": { "amount": "12.34" }
                },
                "adSpend": { "amount": "5.67" }
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body["data"]["activity"]["userErrors"], json!([]));
    assert_eq!(
        response.body["data"]["activity"]["marketingActivity"]["budget"]["total"],
        json!({ "amount": "12.34", "currencyCode": "CAD" })
    );
    assert_eq!(
        response.body["data"]["activity"]["marketingActivity"]["adSpend"],
        json!({ "amount": "5.67", "currencyCode": "CAD" })
    );
}

#[test]
fn marketing_external_activity_update_and_upsert_reject_tactic_change_from_storefront_app() {
    let mut proxy = snapshot_proxy();
    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityUpdateCurrencyAndTacticGuardsSetup(
          $updateInput: MarketingActivityCreateExternalInput!
          $upsertInput: MarketingActivityCreateExternalInput!
        ) {
          updateSeed: marketingActivityCreateExternal(input: $updateInput) {
            marketingActivity { id title tactic remoteId }
            userErrors { field message code }
          }
          upsertSeed: marketingActivityCreateExternal(input: $upsertInput) {
            marketingActivity { id title tactic remoteId }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "updateInput": {"title": "Storefront update seed", "remoteId": "storefront-update-seed", "status": "ACTIVE", "remoteUrl": "https://example.com/storefront-update-seed", "tactic": "STOREFRONT_APP", "marketingChannelType": "EMAIL", "utm": {"campaign": "storefront-update-seed", "source": "email", "medium": "newsletter"}},
            "upsertInput": {"title": "Storefront upsert seed", "remoteId": "storefront-upsert-seed", "status": "ACTIVE", "remoteUrl": "https://example.com/storefront-upsert-seed", "tactic": "STOREFRONT_APP", "marketingChannelType": "EMAIL", "utm": {"campaign": "storefront-upsert-seed", "source": "email", "medium": "newsletter"}}
        }),
    ));
    assert_eq!(setup.body["data"]["updateSeed"]["userErrors"], json!([]));
    assert_eq!(setup.body["data"]["upsertSeed"]["userErrors"], json!([]));
    let update_activity_id = setup.body["data"]["updateSeed"]["marketingActivity"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let upsert_activity_id = setup.body["data"]["upsertSeed"]["marketingActivity"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let guards = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityUpdateCurrencyAndTacticGuardsFromStorefront(
          $updateActivityId: ID!
          $updateInput: MarketingActivityUpdateExternalInput!
          $upsertInput: MarketingActivityUpsertExternalInput!
        ) {
          updateFromStorefront: marketingActivityUpdateExternal(marketingActivityId: $updateActivityId, input: $updateInput) {
            marketingActivity { id title tactic }
            userErrors { field message code }
          }
          upsertFromStorefront: marketingActivityUpsertExternal(input: $upsertInput) {
            marketingActivity { id title tactic }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "updateActivityId": update_activity_id,
            "updateInput": {"title": "Should not stage update", "tactic": "NEWSLETTER"},
            "upsertInput": {"remoteId": "storefront-upsert-seed", "title": "Should not stage upsert", "tactic": "NEWSLETTER"}
        }),
    ));
    let expected_error = json!([{
        "field": ["input"],
        "message": "You can not update an activity tactic from STOREFRONT_APP.",
        "code": "CANNOT_UPDATE_TACTIC_IF_ORIGINALLY_STOREFRONT_APP"
    }]);
    assert_eq!(
        guards.body["data"]["updateFromStorefront"],
        json!({"marketingActivity": null, "userErrors": expected_error})
    );
    assert_eq!(
        guards.body["data"]["upsertFromStorefront"],
        json!({"marketingActivity": null, "userErrors": expected_error})
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MarketingActivityRead($updateActivityId: ID!, $upsertActivityId: ID!) {
          updateSeed: marketingActivity(id: $updateActivityId) { title tactic }
          upsertSeed: marketingActivity(id: $upsertActivityId) { title tactic }
        }
        "#,
        json!({
            "updateActivityId": update_activity_id,
            "upsertActivityId": upsert_activity_id
        }),
    ));
    assert_eq!(
        read.body["data"]["updateSeed"],
        json!({"title": "Storefront update seed", "tactic": "STOREFRONT_APP"})
    );
    assert_eq!(
        read.body["data"]["upsertSeed"],
        json!({"title": "Storefront upsert seed", "tactic": "STOREFRONT_APP"})
    );
}

#[test]
fn marketing_per_app_scoping_keeps_external_activity_owned_by_request_app() {
    let mut proxy = snapshot_proxy();
    let mut create = json_graphql_request(
        r#"
        mutation MarketingActivityPerAppCreate {
          createExternal: marketingActivityCreateExternal(input: { title: "Per App Campaign", remoteId: "campaign-1", status: ACTIVE, tactic: NEWSLETTER, marketingChannelType: EMAIL, remoteUrl: "https://example.com/per-app", budget: { budgetType: DAILY, total: { amount: "100.00", currencyCode: USD } }, urlParameterValue: "utm_campaign=per-app-a", utm: { campaign: "per-app-a", source: "newsletter", medium: "email" } }) {
            marketingActivity { id title remoteId }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    create.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "app-a".to_string(),
    );
    let create = proxy.process_request(create);
    let activity_id = create.body["data"]["createExternal"]["marketingActivity"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"]["createExternal"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["createExternal"]["marketingActivity"]["title"],
        json!("Per App Campaign")
    );

    let mut app_b_update = json_graphql_request(
        r#"
        mutation MarketingActivityPerAppUpdate {
          updateExternal: marketingActivityUpdateExternal(remoteId: "campaign-1", input: { title: "App B Attempted Update" }) {
            marketingActivity { id title }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    app_b_update.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "app-b".to_string(),
    );
    let app_b_update = proxy.process_request(app_b_update);
    assert_eq!(
        app_b_update.body["data"]["updateExternal"],
        json!({"marketingActivity": null, "userErrors": [{"field": null, "message": "Marketing activity does not exist.", "code": "MARKETING_ACTIVITY_DOES_NOT_EXIST"}]})
    );

    let mut app_b_delete = json_graphql_request(
        r#"
        mutation MarketingActivityPerAppDelete {
          deleteExternal: marketingActivityDeleteExternal(remoteId: "campaign-1") {
            deletedMarketingActivityId
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    app_b_delete.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "app-b".to_string(),
    );
    let app_b_delete = proxy.process_request(app_b_delete);
    assert_eq!(
        app_b_delete.body["data"]["deleteExternal"],
        json!({"deletedMarketingActivityId": null, "userErrors": [{"field": null, "message": "Marketing activity does not exist.", "code": "MARKETING_ACTIVITY_DOES_NOT_EXIST"}]})
    );

    let mut app_b_engagement = json_graphql_request(
        r#"
        mutation MarketingActivityPerAppEngagement {
          engagementCreate: marketingEngagementCreate(remoteId: "campaign-1", marketingEngagement: { occurredOn: "2026-05-06", utcOffset: "+00:00", isCumulative: false, adSpend: { amount: "10.00", currencyCode: EUR } }) {
            marketingEngagement { occurredOn }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    app_b_engagement.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "app-b".to_string(),
    );
    let app_b_engagement = proxy.process_request(app_b_engagement);
    assert_eq!(
        app_b_engagement.body["data"]["engagementCreate"],
        json!({"marketingEngagement": null, "userErrors": [{"field": null, "message": "Marketing activity does not exist.", "code": "MARKETING_ACTIVITY_DOES_NOT_EXIST"}]})
    );

    let mut app_b_delete_all = json_graphql_request(
        r#"
        mutation MarketingActivityPerAppDeleteAll {
          deleteAllExternal: marketingActivitiesDeleteAllExternal { job { done } userErrors { field message code } }
        }
        "#,
        json!({}),
    );
    app_b_delete_all.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "app-b".to_string(),
    );
    let app_b_delete_all = proxy.process_request(app_b_delete_all);
    assert_eq!(
        app_b_delete_all.body["data"]["deleteAllExternal"],
        json!({"job": {"done": false}, "userErrors": []})
    );

    let mut app_a_read = json_graphql_request(
        r#"
        query MarketingActivityPerAppRead($activityId: ID!) { marketingActivity(id: $activityId) { title remoteId } }
        "#,
        json!({"activityId": activity_id.clone()}),
    );
    app_a_read.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "app-a".to_string(),
    );
    let app_a_read = proxy.process_request(app_a_read);
    assert_eq!(
        app_a_read.body["data"]["marketingActivity"],
        json!({"title": "Per App Campaign", "remoteId": "campaign-1"})
    );

    let mut app_a_delete = json_graphql_request(
        r#"
        mutation MarketingActivityPerAppOwnerDelete {
          deleteExternal: marketingActivityDeleteExternal(remoteId: "campaign-1") {
            deletedMarketingActivityId
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    app_a_delete.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "app-a".to_string(),
    );
    let app_a_delete = proxy.process_request(app_a_delete);
    assert_eq!(
        app_a_delete.body["data"]["deleteExternal"],
        json!({"deletedMarketingActivityId": activity_id.clone(), "userErrors": []})
    );

    let mut app_a_read_after_delete = json_graphql_request(
        r#"
        query MarketingActivityPerAppReadAfterDelete($activityId: ID!) { marketingActivity(id: $activityId) { title remoteId } }
        "#,
        json!({"activityId": activity_id}),
    );
    app_a_read_after_delete.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "app-a".to_string(),
    );
    let app_a_read_after_delete = proxy.process_request(app_a_read_after_delete);
    assert_eq!(
        app_a_read_after_delete.body["data"]["marketingActivity"],
        Value::Null
    );
}

#[test]
fn marketing_external_activity_uses_request_app_custom_channel_and_tracking_values() {
    let mut proxy = snapshot_proxy();
    let mut create = json_graphql_request(
        r#"
        mutation MarketingActivityRequestIdentityAndTracking {
          createExternal: marketingActivityCreateExternal(input: {
            title: "Social Launch",
            remoteId: "social-remote-1",
            status: ACTIVE,
            tactic: AD,
            marketingChannelType: SEARCH,
            channelHandle: "social-feed",
            remoteUrl: "https://example.com/social-launch",
            previewUrl: "https://example.com/social-preview",
            utm: { campaign: "social-campaign", source: "social", medium: "paid" }
          }) {
            marketingActivity {
              id
              remoteId
              app { id title }
              utmParameters { campaign source medium }
              marketingEvent {
                id
                remoteId
                channelHandle
                utmCampaign
                utmSource
                utmMedium
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    create.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let create = proxy.process_request(create);
    let created = &create.body["data"]["createExternal"]["marketingActivity"];
    let activity_id = created["id"].as_str().expect("activity id");
    let activity_tail = activity_id
        .rsplit('/')
        .next()
        .and_then(|tail| tail.parse::<u64>().ok())
        .expect("numeric marketing activity id");
    let assumed_event_id = format!("gid://shopify/MarketingEvent/{}", activity_tail + 1);

    assert_eq!(
        create.body["data"]["createExternal"]["userErrors"],
        json!([])
    );
    assert_eq!(
        created,
        &json!({
            "id": activity_id,
            "remoteId": "social-remote-1",
            "app": { "id": "gid://shopify/App/347082227713", "title": "shopify-draft-proxy" },
            "utmParameters": { "campaign": "social-campaign", "source": "social", "medium": "paid" },
            "marketingEvent": {
                "id": created["marketingEvent"]["id"],
                "remoteId": "social-remote-1",
                "channelHandle": "social-feed",
                "utmCampaign": "social-campaign",
                "utmSource": "social",
                "utmMedium": "paid"
            }
        })
    );
    assert_ne!(
        created["marketingEvent"]["id"],
        json!(assumed_event_id),
        "marketing event ids must be allocated independently from activity ids"
    );
}

#[test]
fn marketing_external_activity_app_title_uses_installed_app_model() {
    let mut proxy = snapshot_proxy();
    let app_id = "gid://shopify/App/347082227713";

    let mut observe_app = json_graphql_request(
        r#"
        query ObserveMarketingApp {
          currentAppInstallation {
            app { id title handle }
          }
        }
        "#,
        json!({}),
    );
    observe_app.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    observe_app.headers.insert(
        "x-shopify-draft-proxy-app-title".to_string(),
        "Hermes Marketing".to_string(),
    );
    observe_app.headers.insert(
        "x-shopify-draft-proxy-app-handle".to_string(),
        "hermes-marketing".to_string(),
    );
    let observed_app = proxy.process_request(observe_app);
    assert_eq!(
        observed_app.body["data"]["currentAppInstallation"]["app"],
        json!({ "id": app_id, "title": "Hermes Marketing", "handle": "hermes-marketing" })
    );

    let mut create = json_graphql_request(
        r#"
        mutation CreateMarketingActivityForInstalledApp {
          createExternal: marketingActivityCreateExternal(input: {
            title: "Installed app campaign",
            remoteId: "installed-app-campaign",
            status: ACTIVE,
            tactic: NEWSLETTER,
            marketingChannelType: EMAIL,
            remoteUrl: "https://example.com/installed-app-campaign",
            utm: { campaign: "installed-app-campaign", source: "email", medium: "newsletter" }
          }) {
            marketingActivity { id app { id title } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    create.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let create = proxy.process_request(create);
    assert_eq!(
        create.body["data"]["createExternal"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["createExternal"]["marketingActivity"]["app"],
        json!({ "id": app_id, "title": "Hermes Marketing" })
    );

    let activity_id = create.body["data"]["createExternal"]["marketingActivity"]["id"]
        .as_str()
        .expect("created activity id")
        .to_string();
    let mut read = json_graphql_request(
        r#"
        query ReadMarketingActivityApp($id: ID!) {
          marketingActivity(id: $id) {
            app { id title }
          }
        }
        "#,
        json!({ "id": activity_id }),
    );
    read.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let read = proxy.process_request(read);
    assert_eq!(
        read.body["data"]["marketingActivity"]["app"],
        json!({ "id": app_id, "title": "Hermes Marketing" })
    );
}

#[test]
fn marketing_engagement_currency_validation_matches_shopify_error_codes() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingEngagementCurrencyValidation($activityInput: MarketingActivityCreateExternalInput!, $remoteId: String!, $activityId: ID!, $mismatchedInputEngagement: MarketingEngagementInput!, $activityCurrencyMismatchEngagement: MarketingEngagementInput!, $remoteActivityCurrencyMismatchEngagement: MarketingEngagementInput!) {
          createActivity: marketingActivityCreateExternal(input: $activityInput) { marketingActivity { id } userErrors { field message code } }
          inputMismatchByRemoteId: marketingEngagementCreate(remoteId: $remoteId, marketingEngagement: $mismatchedInputEngagement) { marketingEngagement { occurredOn } userErrors { field message code } }
          activityMismatchById: marketingEngagementCreate(marketingActivityId: $activityId, marketingEngagement: $activityCurrencyMismatchEngagement) { marketingEngagement { occurredOn } userErrors { field message code } }
          activityMismatchByRemoteId: marketingEngagementCreate(remoteId: $remoteId, marketingEngagement: $remoteActivityCurrencyMismatchEngagement) { marketingEngagement { occurredOn } userErrors { field message code } }
        }
        "#,
        json!({
            "activityInput": {"title": "HAR-684 Currency Validation Campaign", "remoteId": "har-684-currency-validation", "status": "ACTIVE", "remoteUrl": "https://example.com/har-684-currency-validation", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "budget": {"budgetType": "DAILY", "total": {"amount": "100.00", "currencyCode": "USD"}}, "utm": {"campaign": "har-684-currency-validation", "source": "newsletter", "medium": "email"}},
            "remoteId": "har-684-currency-validation",
            "activityId": "gid://shopify/MarketingActivity/1",
            "mismatchedInputEngagement": {"occurredOn": "2026-04-01", "isCumulative": false, "utcOffset": "+00:00", "adSpend": {"amount": "10.00", "currencyCode": "USD"}, "sales": {"amount": "30.00", "currencyCode": "EUR"}},
            "activityCurrencyMismatchEngagement": {"occurredOn": "2026-04-02", "isCumulative": false, "utcOffset": "+00:00", "adSpend": {"amount": "10.00", "currencyCode": "EUR"}},
            "remoteActivityCurrencyMismatchEngagement": {"occurredOn": "2026-04-03", "isCumulative": false, "utcOffset": "+00:00", "sales": {"amount": "30.00", "currencyCode": "EUR"}}
        }),
    ));

    assert_eq!(
        response.body["data"]["inputMismatchByRemoteId"]["userErrors"],
        json!([{ "field": ["marketingEngagement"], "message": "Currency codes in the marketing engagement input do not match.", "code": "CURRENCY_CODE_MISMATCH_INPUT" }])
    );
    assert_eq!(
        response.body["data"]["inputMismatchByRemoteId"]["marketingEngagement"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["activityMismatchById"]["userErrors"],
        json!([{ "field": ["marketingEngagement"], "message": "Marketing activity currency code does not match the currency code in the marketing engagement input.", "code": "MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH" }])
    );
    assert_eq!(
        response.body["data"]["activityMismatchById"]["marketingEngagement"],
        json!(null)
    );
    assert_eq!(
        response.body["data"]["activityMismatchByRemoteId"]["userErrors"],
        json!([{ "field": ["marketingEngagement"], "message": "Marketing activity currency code does not match the currency code in the marketing engagement input.", "code": "MARKETING_ACTIVITY_CURRENCY_CODE_MISMATCH" }])
    );
    assert_eq!(
        response.body["data"]["activityMismatchByRemoteId"]["marketingEngagement"],
        json!(null)
    );
}

#[test]
fn marketing_channel_handles_accept_non_empty_values() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingChannelHandleAcceptance(
          $createInput: MarketingActivityCreateExternalInput!
          $upsertInput: MarketingActivityUpsertExternalInput!
          $engagement: MarketingEngagementInput!
        ) {
          customEngagement: marketingEngagementCreate(channelHandle: "not-a-real-channel", marketingEngagement: $engagement) {
            marketingEngagement { occurredOn }
            userErrors { field message code }
          }
          customCreate: marketingActivityCreateExternal(input: $createInput) {
            marketingActivity { id }
            userErrors { field message code }
          }
          customUpsert: marketingActivityUpsertExternal(input: $upsertInput) {
            marketingActivity { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "createInput": {"title": "Invalid create channel", "remoteId": "invalid-create-channel", "status": "ACTIVE", "remoteUrl": "https://example.com/invalid-create-channel", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "channelHandle": "not-a-real-channel", "utm": {"campaign": "invalid-create-channel", "source": "email", "medium": "newsletter"}},
            "upsertInput": {"title": "Invalid upsert channel", "remoteId": "invalid-upsert-channel", "status": "ACTIVE", "remoteUrl": "https://example.com/invalid-upsert-channel", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "channelHandle": "not-a-real-channel", "utm": {"campaign": "invalid-upsert-channel", "source": "email", "medium": "newsletter"}},
            "engagement": {"occurredOn": "2026-04-01", "isCumulative": false, "utcOffset": "+00:00"}
        }),
    ));

    assert_eq!(
        response.body["data"]["customEngagement"],
        json!({"marketingEngagement": {"occurredOn": "2026-04-01"}, "userErrors": []})
    );
    assert_eq!(
        response.body["data"]["customCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["customUpsert"]["userErrors"],
        json!([])
    );
    assert!(
        response.body["data"]["customCreate"]["marketingActivity"]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("gid://shopify/MarketingActivity/"))
    );
    assert!(
        response.body["data"]["customUpsert"]["marketingActivity"]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("gid://shopify/MarketingActivity/"))
    );
}

#[test]
fn marketing_external_activity_create_validation_reaches_rust_handler() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityCreateExternalValidation(
          $currencyMismatchInput: MarketingActivityCreateExternalInput!
          $utmSeedInput: MarketingActivityCreateExternalInput!
          $duplicateUtmCampaignInput: MarketingActivityCreateExternalInput!
          $urlSeedInput: MarketingActivityCreateExternalInput!
          $duplicateUrlParameterValueInput: MarketingActivityCreateExternalInput!
        ) {
          currencyMismatch: marketingActivityCreateExternal(input: $currencyMismatchInput) { marketingActivity { id } userErrors { field message code } }
          utmSeed: marketingActivityCreateExternal(input: $utmSeedInput) { marketingActivity { id } userErrors { field message code } }
          duplicateUtmCampaign: marketingActivityCreateExternal(input: $duplicateUtmCampaignInput) { marketingActivity { id } userErrors { field message code } }
          urlSeed: marketingActivityCreateExternal(input: $urlSeedInput) { marketingActivity { id } userErrors { field message code } }
          duplicateUrlParameterValue: marketingActivityCreateExternal(input: $duplicateUrlParameterValueInput) { marketingActivity { id } userErrors { field message code } }
        }
        "#,
        json!({
            "currencyMismatchInput": {"title": "Currency mismatch", "remoteId": "currency-mismatch", "status": "ACTIVE", "remoteUrl": "https://example.com/currency", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "currency-mismatch", "source": "email", "medium": "newsletter"}, "budget": {"budgetType": "DAILY", "total": {"amount": "1.00", "currencyCode": "USD"}}, "adSpend": {"amount": "1.00", "currencyCode": "EUR"}},
            "utmSeedInput": {"title": "UTM Seed", "remoteId": "utm-seed", "status": "ACTIVE", "remoteUrl": "https://example.com/utm-seed", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "utm-seed", "source": "email", "medium": "newsletter"}, "urlParameterValue": "utm-seed"},
            "duplicateUtmCampaignInput": {"title": "Duplicate UTM", "remoteId": "utm-duplicate", "status": "ACTIVE", "remoteUrl": "https://example.com/utm-duplicate", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "utm-seed", "source": "email", "medium": "newsletter"}, "urlParameterValue": "utm-duplicate"},
            "urlSeedInput": {"title": "URL Seed", "remoteId": "url-seed", "status": "ACTIVE", "remoteUrl": "https://example.com/url-seed", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "url-seed", "source": "email", "medium": "newsletter"}, "urlParameterValue": "url-seed-param"},
            "duplicateUrlParameterValueInput": {"title": "Duplicate URL", "remoteId": "url-duplicate", "status": "ACTIVE", "remoteUrl": "https://example.com/url-duplicate", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "url-duplicate", "source": "email", "medium": "newsletter"}, "urlParameterValue": "url-seed-param"}
        }),
    ));

    assert_eq!(
        response.body["data"]["currencyMismatch"],
        json!({"marketingActivity": null, "userErrors": [{"field": ["input"], "message": "Currency code is not matching between budget and ad spend", "code": null}]})
    );
    assert_eq!(
        response.body["data"]["duplicateUtmCampaign"],
        json!({"marketingActivity": null, "userErrors": [{"field": ["input"], "message": "Validation failed: Utm campaign has already been taken", "code": null}]})
    );
    assert_eq!(
        response.body["data"]["duplicateUrlParameterValue"],
        json!({"marketingActivity": null, "userErrors": [{"field": ["input"], "message": "Validation failed: Url parameter value has already been taken", "code": null}]})
    );
}

#[test]
fn marketing_external_activity_upsert_create_branch_rejects_currency_and_duplicates() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycle(
          $seedInput: MarketingActivityCreateExternalInput!
          $currencyMismatchInput: MarketingActivityUpsertExternalInput!
          $duplicateUtmCampaignInput: MarketingActivityUpsertExternalInput!
          $duplicateUrlParameterValueInput: MarketingActivityUpsertExternalInput!
        ) {
          seed: marketingActivityCreateExternal(input: $seedInput) { marketingActivity { id } userErrors { field message code } }
          currencyMismatch: marketingActivityUpsertExternal(input: $currencyMismatchInput) { marketingActivity { id } userErrors { field message code } }
          duplicateUtmCampaign: marketingActivityUpsertExternal(input: $duplicateUtmCampaignInput) { marketingActivity { id } userErrors { field message code } }
          duplicateUrlParameterValue: marketingActivityUpsertExternal(input: $duplicateUrlParameterValueInput) { marketingActivity { id } userErrors { field message code } }
        }
        "#,
        json!({
            "seedInput": {"title": "Seed", "remoteId": "upsert-seed", "status": "ACTIVE", "remoteUrl": "https://example.com/upsert-seed", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "upsert-seed", "source": "email", "medium": "newsletter"}, "urlParameterValue": "upsert-seed-param"},
            "currencyMismatchInput": {"title": "Currency mismatch", "remoteId": "upsert-currency", "status": "ACTIVE", "remoteUrl": "https://example.com/upsert-currency", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "upsert-currency", "source": "email", "medium": "newsletter"}, "budget": {"budgetType": "DAILY", "total": {"amount": "1.00", "currencyCode": "USD"}}, "adSpend": {"amount": "1.00", "currencyCode": "EUR"}},
            "duplicateUtmCampaignInput": {"title": "Duplicate UTM", "remoteId": "upsert-utm-duplicate", "status": "ACTIVE", "remoteUrl": "https://example.com/upsert-utm-duplicate", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "upsert-seed", "source": "email", "medium": "newsletter"}, "urlParameterValue": "upsert-utm-duplicate"},
            "duplicateUrlParameterValueInput": {"title": "Duplicate URL", "remoteId": "upsert-url-duplicate", "status": "ACTIVE", "remoteUrl": "https://example.com/upsert-url-duplicate", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "upsert-url-duplicate", "source": "email", "medium": "newsletter"}, "urlParameterValue": "upsert-seed-param"}
        }),
    ));

    assert_eq!(response.body["data"]["seed"]["userErrors"], json!([]));
    assert_eq!(
        response.body["data"]["currencyMismatch"],
        json!({"marketingActivity": null, "userErrors": [{"field": ["input"], "message": "Currency code is not matching between budget and ad spend", "code": null}]})
    );
    assert_eq!(
        response.body["data"]["duplicateUtmCampaign"],
        json!({"marketingActivity": null, "userErrors": [{"field": ["input"], "message": "Validation failed: Utm campaign has already been taken", "code": null}]})
    );
    assert_eq!(
        response.body["data"]["duplicateUrlParameterValue"],
        json!({"marketingActivity": null, "userErrors": [{"field": ["input"], "message": "Validation failed: Url parameter value has already been taken, Url parameter value has already been taken", "code": null}]})
    );
}

#[test]
fn marketing_external_activity_update_and_upsert_reject_immutable_field_changes() {
    let mut proxy = snapshot_proxy();
    let seed = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycle(
          $parentInput: MarketingActivityCreateExternalInput!
          $otherParentInput: MarketingActivityCreateExternalInput!
          $childInput: MarketingActivityCreateExternalInput!
          $utmOnlyInput: MarketingActivityCreateExternalInput!
        ) {
          parent: marketingActivityCreateExternal(input: $parentInput) { marketingActivity { id } userErrors { field message code } }
          otherParent: marketingActivityCreateExternal(input: $otherParentInput) { marketingActivity { id } userErrors { field message code } }
          child: marketingActivityCreateExternal(input: $childInput) { marketingActivity { id title parentRemoteId hierarchyLevel urlParameterValue utmParameters { campaign source medium } marketingEvent { channelHandle } } userErrors { field message code } }
          utmOnly: marketingActivityCreateExternal(input: $utmOnlyInput) { marketingActivity { id } userErrors { field message code } }
        }
        "#,
        json!({
            "parentInput": {"title": "Parent", "remoteId": "guard-parent", "status": "ACTIVE", "remoteUrl": "https://example.com/parent", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "guard-parent", "source": "email", "medium": "newsletter"}, "hierarchyLevel": "CAMPAIGN"},
            "otherParentInput": {"title": "Other parent", "remoteId": "guard-other-parent", "status": "ACTIVE", "remoteUrl": "https://example.com/other-parent", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "guard-other-parent", "source": "email", "medium": "newsletter"}, "hierarchyLevel": "CAMPAIGN"},
            "childInput": {"title": "Child", "remoteId": "guard-child", "status": "ACTIVE", "remoteUrl": "https://example.com/child", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "urlParameterValue": "guard-child-url", "utm": {"campaign": "guard-child", "source": "email", "medium": "newsletter"}, "parentRemoteId": "guard-parent", "hierarchyLevel": "AD"},
            "utmOnlyInput": {"title": "UTM only", "remoteId": "guard-utm-only", "status": "ACTIVE", "remoteUrl": "https://example.com/utm-only", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "guard-utm-only", "source": "email", "medium": "newsletter"}}
        }),
    ));
    assert_eq!(seed.body["data"]["parent"]["userErrors"], json!([]));
    assert_eq!(seed.body["data"]["otherParent"]["userErrors"], json!([]));
    assert_eq!(seed.body["data"]["child"]["userErrors"], json!([]));
    assert_eq!(seed.body["data"]["utmOnly"]["userErrors"], json!([]));

    let order = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycle($input: MarketingActivityUpsertExternalInput!) {
          changed: marketingActivityUpsertExternal(input: $input) {
            marketingActivity { id title }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"title": "Should not stage", "remoteId": "guard-child", "status": "ACTIVE", "remoteUrl": "https://example.com/child", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "urlParameterValue": "changed-url", "utm": {"campaign": "changed-campaign", "source": "email", "medium": "newsletter"}, "channelHandle": "changed-channel"}}),
    ));
    assert_eq!(
        order.body["data"]["changed"],
        json!({"marketingActivity": null, "userErrors": [{"field": ["input"], "message": "Channel handle cannot be modified.", "code": "IMMUTABLE_CHANNEL_HANDLE"}]})
    );

    let update_parent = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycle($remoteId: String!, $input: MarketingActivityUpdateExternalInput!) {
          changed: marketingActivityUpdateExternal(remoteId: $remoteId, input: $input) {
            marketingActivity { id title }
            userErrors { field message code }
          }
        }
        "#,
        json!({"remoteId": "guard-child", "input": {"title": "Should not stage parent", "urlParameterValue": "guard-child-url", "utm": {"campaign": "guard-child", "source": "email", "medium": "newsletter"}, "parentRemoteId": "guard-other-parent", "hierarchyLevel": "AD"}}),
    ));
    assert_eq!(
        update_parent.body["data"]["changed"],
        json!({"marketingActivity": null, "userErrors": [{"field": ["input"], "message": "Parent ID cannot be modified.", "code": "IMMUTABLE_PARENT_ID"}]})
    );

    let invalid_parent = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycle($input: MarketingActivityUpsertExternalInput!) {
          changed: marketingActivityUpsertExternal(input: $input) {
            marketingActivity { id title }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"title": "Should not stage invalid parent", "remoteId": "guard-child", "status": "ACTIVE", "remoteUrl": "https://example.com/child", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "urlParameterValue": "guard-child-url", "utm": {"campaign": "guard-child", "source": "email", "medium": "newsletter"}, "parentRemoteId": "missing-parent", "hierarchyLevel": "AD"}}),
    ));
    assert_eq!(
        invalid_parent.body["data"]["changed"],
        json!({"marketingActivity": null, "userErrors": [{"field": ["input"], "message": "Remote ID does not correspond to an activity.", "code": "INVALID_REMOTE_ID"}]})
    );

    let hierarchy = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycle($input: MarketingActivityUpsertExternalInput!) {
          changed: marketingActivityUpsertExternal(input: $input) {
            marketingActivity { id title }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"title": "Should not stage hierarchy", "remoteId": "guard-child", "status": "ACTIVE", "remoteUrl": "https://example.com/child", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "urlParameterValue": "guard-child-url", "utm": {"campaign": "guard-child", "source": "email", "medium": "newsletter"}, "parentRemoteId": "guard-parent", "hierarchyLevel": "AD_GROUP"}}),
    ));
    assert_eq!(
        hierarchy.body["data"]["changed"],
        json!({"marketingActivity": null, "userErrors": [{"field": ["input"], "message": "Hierarchy level cannot be modified.", "code": "IMMUTABLE_HIERARCHY_LEVEL"}]})
    );

    let omitted_url = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycle($input: MarketingActivityUpsertExternalInput!) {
          changed: marketingActivityUpsertExternal(input: $input) {
            marketingActivity { id title }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"title": "Should not stage omitted URL", "remoteId": "guard-child", "status": "ACTIVE", "remoteUrl": "https://example.com/child", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "guard-child", "source": "email", "medium": "newsletter"}, "parentRemoteId": "guard-parent", "hierarchyLevel": "AD"}}),
    ));
    // Omitting urlParameterValue is NOT a modification: real Shopify only emits
    // IMMUTABLE_URL_PARAMETER when the field is present AND changed (see the
    // recorded `upsert-immutable-url-parameter` capture, which sends a
    // `...-changed` value). An upsert that leaves the field out simply updates
    // the existing activity, returning its established id.
    assert_eq!(
        omitted_url.body["data"]["changed"],
        json!({"marketingActivity": {"id": seed.body["data"]["child"]["marketingActivity"]["id"], "title": "Should not stage omitted URL"}, "userErrors": []})
    );

    let omitted_utm = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycle($remoteId: String!, $input: MarketingActivityUpdateExternalInput!) {
          changed: marketingActivityUpdateExternal(remoteId: $remoteId, input: $input) {
            marketingActivity { id title }
            userErrors { field message code }
          }
        }
        "#,
        json!({"remoteId": "guard-utm-only", "input": {"title": "Should not stage omitted UTM"}}),
    ));
    assert_eq!(
        omitted_utm.body["data"]["changed"],
        json!({"marketingActivity": {"id": seed.body["data"]["utmOnly"]["marketingActivity"]["id"], "title": "Should not stage omitted UTM"}, "userErrors": []})
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MarketingActivityLifecycleRead($remoteIds: [String!]) {
          marketingActivities(first: 10, remoteIds: $remoteIds) { nodes { title remoteId parentRemoteId hierarchyLevel urlParameterValue utmParameters { campaign source medium } } }
        }
        "#,
        json!({"remoteIds": ["guard-child", "guard-utm-only"]}),
    ));
    // The omitted-URL upsert above is a partial update: it changed only the
    // mutable title and left every immutable field pristine. The guard still
    // holds — urlParameterValue/parentRemoteId/hierarchyLevel remain unchanged —
    // so the stored title reflects that last successful upsert.
    assert_eq!(
        read.body["data"]["marketingActivities"]["nodes"][0],
        json!({"title": "Should not stage omitted URL", "remoteId": "guard-child", "parentRemoteId": "guard-parent", "hierarchyLevel": "AD", "urlParameterValue": "guard-child-url", "utmParameters": {"campaign": "guard-child", "source": "email", "medium": "newsletter"}})
    );
    assert_eq!(
        read.body["data"]["marketingActivities"]["nodes"][1]["title"],
        json!("Should not stage omitted UTM")
    );
}

#[test]
fn marketing_external_activity_update_and_upsert_reject_not_external_and_missing_event_records() {
    let mut proxy = snapshot_proxy();
    let seed = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingEngagementCreateValidationOrder($activityInput: MarketingActivityUpdateInput!, $externalInput: MarketingActivityCreateExternalInput!) {
          native: marketingActivityUpdate(input: $activityInput) { marketingActivity { id title isExternal marketingEvent { id } } userErrors { field message } }
          external: marketingActivityCreateExternal(input: $externalInput) { marketingActivity { id title remoteId } userErrors { field message code } }
        }
        "#,
        json!({
            "activityInput": {"id": "gid://shopify/MarketingActivity/native-no-event", "title": "Native no event", "remoteId": "native-local", "status": "ACTIVE"},
            "externalInput": {"title": "External", "remoteId": "eventless-remote", "status": "ACTIVE", "remoteUrl": "https://example.com/eventless", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "eventless-remote", "source": "email", "medium": "newsletter"}}
        }),
    ));
    assert_eq!(seed.body["data"]["native"]["userErrors"], json!([]));
    assert_eq!(seed.body["data"]["external"]["userErrors"], json!([]));
    let external_id = seed.body["data"]["external"]["marketingActivity"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let not_external_update = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycle($marketingActivityId: ID!, $input: MarketingActivityUpdateExternalInput!) {
          changed: marketingActivityUpdateExternal(marketingActivityId: $marketingActivityId, input: $input) {
            marketingActivity { id title }
            userErrors { field message code }
          }
        }
        "#,
        json!({"marketingActivityId": "gid://shopify/MarketingActivity/native-no-event", "input": {"title": "Should not stage native"}}),
    ));
    assert_eq!(
        not_external_update.body["data"]["changed"],
        json!({"marketingActivity": null, "userErrors": [{"field": null, "message": "Marketing activity is not external.", "code": "ACTIVITY_NOT_EXTERNAL"}]})
    );

    let not_external_upsert = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingActivityLifecycle($input: MarketingActivityUpsertExternalInput!) {
          changed: marketingActivityUpsertExternal(input: $input) {
            marketingActivity { id title }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"title": "Should not stage native upsert", "remoteId": "native-local", "status": "ACTIVE", "remoteUrl": "https://example.com/native", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "native-local", "source": "email", "medium": "newsletter"}}}),
    ));
    assert_eq!(
        not_external_upsert.body["data"]["changed"],
        json!({"marketingActivity": null, "userErrors": [{"field": null, "message": "Marketing activity is not external.", "code": "ACTIVITY_NOT_EXTERNAL"}]})
    );

    assert!(external_id.starts_with("gid://shopify/MarketingActivity/"));
}

#[test]
fn marketing_engagement_create_validation_order_and_missing_event_reach_rust_handler() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingEngagementCreateValidationOrder(
          $activityInput: MarketingActivityUpdateInput!
          $missingActivityId: ID!
          $missingRemoteId: String!
          $currencyMismatchEngagement: MarketingEngagementInput!
          $validEngagement: MarketingEngagementInput!
        ) {
          activityWithoutEvent: marketingActivityUpdate(input: $activityInput) { marketingActivity { id marketingEvent { id } } userErrors { field message } }
          unknownRemoteCurrency: marketingEngagementCreate(remoteId: $missingRemoteId, marketingEngagement: $currencyMismatchEngagement) { marketingEngagement { occurredOn } userErrors { field message code } }
          missingActivity: marketingEngagementCreate(marketingActivityId: $missingActivityId, marketingEngagement: $validEngagement) { marketingEngagement { occurredOn } userErrors { field message code } }
          missingEvent: marketingEngagementCreate(marketingActivityId: "gid://shopify/MarketingActivity/1", marketingEngagement: $validEngagement) { marketingEngagement { occurredOn } userErrors { field message code } }
        }
        "#,
        json!({
            "activityInput": {"id": "gid://shopify/MarketingActivity/1", "title": "Native activity without event", "status": "ACTIVE"},
            "missingActivityId": "gid://shopify/MarketingActivity/999999999999",
            "missingRemoteId": "missing-remote",
            "currencyMismatchEngagement": {"occurredOn": "2026-04-01", "isCumulative": false, "utcOffset": "+00:00", "adSpend": {"amount": "1.00", "currencyCode": "USD"}, "sales": {"amount": "2.00", "currencyCode": "EUR"}},
            "validEngagement": {"occurredOn": "2026-04-01", "isCumulative": false, "utcOffset": "+00:00", "adSpend": {"amount": "1.00", "currencyCode": "USD"}}
        }),
    ));

    assert_eq!(
        response.body["data"]["unknownRemoteCurrency"],
        json!({"marketingEngagement": null, "userErrors": [{"field": ["marketingEngagement"], "message": "Currency codes in the marketing engagement input do not match.", "code": "CURRENCY_CODE_MISMATCH_INPUT"}]})
    );
    assert_eq!(
        response.body["data"]["missingActivity"],
        json!({"marketingEngagement": null, "userErrors": [{"field": null, "message": "Marketing activity does not exist.", "code": "MARKETING_ACTIVITY_DOES_NOT_EXIST"}]})
    );
    assert_eq!(
        response.body["data"]["missingEvent"],
        json!({"marketingEngagement": null, "userErrors": [{"field": null, "message": "Marketing event does not exist.", "code": "MARKETING_EVENT_DOES_NOT_EXIST"}]})
    );
}

#[test]
fn marketing_engagements_delete_validates_selectors_and_channel_handles() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingEngagementLifecycle {
          seedEmail: marketingActivityCreateExternal(input: { title: "Email channel", remoteId: "delete-email-channel", status: ACTIVE, remoteUrl: "https://example.com/delete-email", tactic: NEWSLETTER, marketingChannelType: EMAIL, channelHandle: "email", utm: { campaign: "delete-email-channel", source: "newsletter", medium: "email" } }) {
            marketingActivity { id marketingEvent { channelHandle } }
            userErrors { field message code }
          }
          bothSelectors: marketingEngagementsDelete(channelHandle: "email", deleteEngagementsForAllChannels: true) {
            result
            userErrors { field message code }
          }
          missingSelector: marketingEngagementsDelete {
            result
            userErrors { field message code }
          }
          unknownChannel: marketingEngagementsDelete(channelHandle: "unknown-channel") {
            result
            userErrors { field message code }
          }
          singleChannel: marketingEngagementsDelete(channelHandle: "email") {
            result
            userErrors { field message code }
          }
          allChannels: marketingEngagementsDelete(deleteEngagementsForAllChannels: true) {
            result
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.body["data"]["seedEmail"]["userErrors"], json!([]));
    assert_eq!(
        response.body["data"]["bothSelectors"],
        json!({"result": null, "userErrors": [{"field": null, "message": "Either the channel_handle or delete_engagements_for_all_channels must be provided when deleting a marketing engagement.", "code": "INVALID_DELETE_ENGAGEMENTS_ARGUMENTS"}]})
    );
    assert_eq!(
        response.body["data"]["missingSelector"],
        json!({"result": null, "userErrors": [{"field": null, "message": "Either the channel_handle or delete_engagements_for_all_channels must be provided when deleting a marketing engagement.", "code": "INVALID_DELETE_ENGAGEMENTS_ARGUMENTS"}]})
    );
    assert_eq!(
        response.body["data"]["unknownChannel"],
        json!({"result": "Engagement data associated to channel handle 'unknown-channel' marked for deletion", "userErrors": []})
    );
    assert_eq!(
        response.body["data"]["singleChannel"],
        json!({"result": "Engagement data associated to channel handle 'email' marked for deletion", "userErrors": []})
    );
    assert_eq!(
        response.body["data"]["allChannels"],
        json!({"result": "Engagement data marked for deletion for 1 channel(s)", "userErrors": []})
    );

    let mut unowned_delete = json_graphql_request(
        r#"
        mutation MarketingEngagementLifecycle {
          unownedChannel: marketingEngagementsDelete(channelHandle: "email") {
            result
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    );
    unowned_delete.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "other-app".to_string(),
    );
    let unowned_delete = proxy.process_request(unowned_delete);
    assert_eq!(
        unowned_delete.body["data"]["unownedChannel"],
        json!({"result": "Engagement data associated to channel handle 'email' marked for deletion", "userErrors": []})
    );
}

#[test]
fn marketing_native_activity_lifecycle_stages_create_update_and_invalid_extension_error() {
    let mut proxy = snapshot_proxy();
    let create_response = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingNativeActivityCreate($createInput: MarketingActivityCreateInput!, $invalidExtensionInput: MarketingActivityCreateInput!) {
          invalidExtension: marketingActivityCreate(input: $createInput) {
            marketingActivity { id title status statusLabel isExternal inMainWorkflowVersion urlParameterValue utmParameters { campaign source medium } budget { budgetType total { amount currencyCode } } marketingEvent { id } }
            redirectPath
            userErrors { field message }
          }
          missingExtension: marketingActivityCreate(input: $invalidExtensionInput) {
            marketingActivity { id title }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "createInput": {"marketingActivityExtensionId": "gid://shopify/MarketingActivityExtension/local-native-extension", "marketingActivityTitle": "Native Activity Draft", "status": "DRAFT", "urlParameterValue": "utm_campaign=native-draft", "utm": {"campaign": "native-draft", "source": "email", "medium": "newsletter"}, "budget": {"budgetType": "DAILY", "total": {"amount": "12.34", "currencyCode": "USD"}}},
            "invalidExtensionInput": {"marketingActivityExtensionId": "gid://shopify/MarketingActivityExtension/00000000-0000-0000-0000-000000000000", "status": "DRAFT"}
        }),
    ));
    assert_eq!(
        create_response.body["data"]["invalidExtension"]["userErrors"],
        json!([])
    );
    let created = &create_response.body["data"]["invalidExtension"]["marketingActivity"];
    let created_id = created["id"]
        .as_str()
        .expect("native create id")
        .to_string();
    assert!(created_id.starts_with("gid://shopify/MarketingActivity/"));
    assert_ne!(created_id, "gid://shopify/MarketingActivity/1");
    assert_eq!(
        created,
        &json!({"id": created_id.clone(), "title": "Native Activity Draft", "status": "DRAFT", "statusLabel": "DRAFT", "isExternal": false, "inMainWorkflowVersion": true, "urlParameterValue": "utm_campaign=native-draft", "utmParameters": {"campaign": "native-draft", "source": "email", "medium": "newsletter"}, "budget": {"budgetType": "DAILY", "total": {"amount": "12.34", "currencyCode": "USD"}}, "marketingEvent": null})
    );
    assert_eq!(
        create_response.body["data"]["missingExtension"],
        json!({"marketingActivity": null, "userErrors": [{ "field": ["input", "marketingActivityExtensionId"], "message": "Could not find the marketing extension" }]})
    );

    let update_response = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingNativeActivityUpdate($updateInput: MarketingActivityUpdateInput!) {
          updateNative: marketingActivityUpdate(input: $updateInput) {
            marketingActivity { id title status statusLabel isExternal inMainWorkflowVersion urlParameterValue utmParameters { campaign source medium } budget { budgetType total { amount currencyCode } } adSpend { amount currencyCode } marketingEvent { id } }
            redirectPath
            userErrors { field message }
          }
        }
        "#,
        json!({
            "updateInput": {"id": created_id.clone(), "title": "Native Activity Updated", "status": "ACTIVE", "urlParameterValue": "utm_campaign=native-updated", "utm": {"campaign": "native-updated", "source": "sms", "medium": "message"}, "budget": {"budgetType": "LIFETIME", "total": {"amount": "98.76", "currencyCode": "USD"}}, "adSpend": {"amount": "7.89", "currencyCode": "USD"}}
        }),
    ));
    assert_eq!(
        update_response.body["data"]["updateNative"]["marketingActivity"],
        json!({"id": created_id.clone(), "title": "Native Activity Updated", "status": "ACTIVE", "statusLabel": "Sending", "isExternal": false, "inMainWorkflowVersion": true, "urlParameterValue": "utm_campaign=native-updated", "utmParameters": {"campaign": "native-updated", "source": "sms", "medium": "message"}, "budget": {"budgetType": "LIFETIME", "total": {"amount": "98.76", "currencyCode": "USD"}}, "adSpend": {"amount": "7.89", "currencyCode": "USD"}, "marketingEvent": null})
    );
    assert_eq!(
        update_response.body["data"]["updateNative"]["redirectPath"],
        json!("/admin/marketing")
    );
    assert_eq!(
        update_response.body["data"]["updateNative"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MarketingNativeActivityRead($activityId: ID!) {
          marketingActivity(id: $activityId) { id title status statusLabel isExternal inMainWorkflowVersion urlParameterValue utmParameters { campaign source medium } budget { budgetType total { amount currencyCode } } adSpend { amount currencyCode } marketingEvent { id } }
          marketingActivities(first: 5, marketingActivityIds: [$activityId]) { nodes { id title status isExternal utmParameters { campaign source medium } budget { total { amount currencyCode } } marketingEvent { id } } }
        }
        "#,
        json!({"activityId": created_id.clone()}),
    ));
    assert_eq!(
        read.body["data"]["marketingActivity"],
        json!({"id": created_id.clone(), "title": "Native Activity Updated", "status": "ACTIVE", "statusLabel": "Sending", "isExternal": false, "inMainWorkflowVersion": true, "urlParameterValue": "utm_campaign=native-updated", "utmParameters": {"campaign": "native-updated", "source": "sms", "medium": "message"}, "budget": {"budgetType": "LIFETIME", "total": {"amount": "98.76", "currencyCode": "USD"}}, "adSpend": {"amount": "7.89", "currencyCode": "USD"}, "marketingEvent": null})
    );
    assert_eq!(
        read.body["data"]["marketingActivities"]["nodes"][0],
        json!({"id": created_id, "title": "Native Activity Updated", "status": "ACTIVE", "isExternal": false, "utmParameters": {"campaign": "native-updated", "source": "sms", "medium": "message"}, "budget": {"total": {"amount": "98.76", "currencyCode": "USD"}}, "marketingEvent": null})
    );

    assert_eq!(
        read.body["data"]["marketingActivity"]
            .to_string()
            .contains("HAR-"),
        false
    );
}

#[test]
fn marketing_activity_delete_external_enforces_resolution_external_and_child_guards() {
    let mut proxy = snapshot_proxy();
    let mut setup = json_graphql_request(
        r#"
        mutation MarketingActivityDeleteExternalGuardsSetup(
          $nativeInput: MarketingActivityUpdateInput!
          $externalInput: MarketingActivityCreateExternalInput!
          $parentInput: MarketingActivityCreateExternalInput!
          $childInput: MarketingActivityCreateExternalInput!
        ) {
          native: marketingActivityUpdate(input: $nativeInput) {
            marketingActivity { id isExternal }
            userErrors { field message  }
          }
          external: marketingActivityCreateExternal(input: $externalInput) {
            marketingActivity { id remoteId isExternal }
            userErrors { field message code }
          }
          parent: marketingActivityCreateExternal(input: $parentInput) {
            marketingActivity { id remoteId isExternal }
            userErrors { field message code }
          }
          child: marketingActivityCreateExternal(input: $childInput) {
            marketingActivity { id remoteId parentRemoteId isExternal }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "nativeInput": {"id": "gid://shopify/MarketingActivity/1001", "title": "Native Activity", "status": "ACTIVE"},
            "externalInput": {"title": "External Activity", "remoteId": "delete-guard-external", "status": "ACTIVE", "remoteUrl": "https://example.com/delete-guard-external", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "utm": {"campaign": "delete-guard-external", "source": "email", "medium": "newsletter"}},
            "parentInput": {"title": "Parent Activity", "remoteId": "delete-guard-parent", "status": "ACTIVE", "remoteUrl": "https://example.com/delete-guard-parent", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "hierarchyLevel": "CAMPAIGN", "utm": {"campaign": "delete-guard-parent", "source": "email", "medium": "newsletter"}},
            "childInput": {"title": "Child Activity", "remoteId": "delete-guard-child", "parentRemoteId": "delete-guard-parent", "status": "ACTIVE", "remoteUrl": "https://example.com/delete-guard-child", "tactic": "NEWSLETTER", "marketingChannelType": "EMAIL", "hierarchyLevel": "AD", "utm": {"campaign": "delete-guard-child", "source": "email", "medium": "newsletter"}}
        }),
    );
    setup.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "app-a".to_string(),
    );
    let setup = proxy.process_request(setup);
    assert_eq!(setup.body["data"]["native"]["userErrors"], json!([]));
    assert_eq!(setup.body["data"]["external"]["userErrors"], json!([]));
    assert_eq!(setup.body["data"]["parent"]["userErrors"], json!([]));
    assert_eq!(setup.body["data"]["child"]["userErrors"], json!([]));
    let external_id = setup.body["data"]["external"]["marketingActivity"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let parent_id = setup.body["data"]["parent"]["marketingActivity"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut guards = json_graphql_request(
        r#"
        mutation MarketingActivityDeleteExternalGuards(
          $unknownId: ID!
          $nativeId: ID!
          $parentId: ID!
          $externalId: ID!
        ) {
          noSelector: marketingActivityDeleteExternal {
            deletedMarketingActivityId
            userErrors { field message code }
          }
          unknownById: marketingActivityDeleteExternal(marketingActivityId: $unknownId) {
            deletedMarketingActivityId
            userErrors { field message code }
          }
          missingRemote: marketingActivityDeleteExternal(remoteId: "missing-delete-guard-remote") {
            deletedMarketingActivityId
            userErrors { field message code }
          }
          nativeById: marketingActivityDeleteExternal(marketingActivityId: $nativeId) {
            deletedMarketingActivityId
            userErrors { field message code }
          }
          parentById: marketingActivityDeleteExternal(id: $parentId) {
            deletedMarketingActivityId
            userErrors { field message code }
          }
          validExternal: marketingActivityDeleteExternal(marketingActivityId: $externalId) {
            deletedMarketingActivityId
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "unknownId": "gid://shopify/MarketingActivity/999999999999",
            "nativeId": "gid://shopify/MarketingActivity/1001",
            "parentId": parent_id,
            "externalId": external_id
        }),
    );
    guards.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "app-a".to_string(),
    );
    let guards = proxy.process_request(guards);
    assert_eq!(
        guards.body["data"]["noSelector"],
        json!({"deletedMarketingActivityId": null, "userErrors": [{"field": null, "message": "Either the marketing activity ID or remote ID must be provided for the activity to be deleted.", "code": "INVALID_DELETE_ACTIVITY_EXTERNAL_ARGUMENTS"}]})
    );
    assert_eq!(
        guards.body["data"]["unknownById"],
        json!({"deletedMarketingActivityId": null, "userErrors": [{"field": null, "message": "Marketing activity does not exist.", "code": "MARKETING_ACTIVITY_DOES_NOT_EXIST"}]})
    );
    assert_eq!(
        guards.body["data"]["missingRemote"],
        json!({"deletedMarketingActivityId": null, "userErrors": [{"field": null, "message": "Marketing activity does not exist.", "code": "MARKETING_ACTIVITY_DOES_NOT_EXIST"}]})
    );
    assert_eq!(
        guards.body["data"]["nativeById"],
        json!({"deletedMarketingActivityId": null, "userErrors": [{"field": null, "message": "The marketing activity must be an external activity.", "code": "ACTIVITY_NOT_EXTERNAL"}]})
    );
    assert_eq!(
        guards.body["data"]["parentById"],
        json!({"deletedMarketingActivityId": null, "userErrors": [{"field": null, "message": "This activity has child activities and thus cannot be deleted. Child activities must be deleted before a parent activity.", "code": "CANNOT_DELETE_ACTIVITY_WITH_CHILD_EVENTS"}]})
    );
    assert_eq!(
        guards.body["data"]["validExternal"],
        json!({"deletedMarketingActivityId": external_id, "userErrors": []})
    );

    let mut read = json_graphql_request(
        r#"
        query MarketingActivityRead($nativeId: ID!, $parentId: ID!, $externalId: ID!) {
          native: marketingActivity(id: $nativeId) { id isExternal }
          parent: marketingActivity(id: $parentId) { id remoteId }
          external: marketingActivity(id: $externalId) { id }
        }
        "#,
        json!({
            "nativeId": "gid://shopify/MarketingActivity/1001",
            "parentId": parent_id,
            "externalId": external_id
        }),
    );
    read.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "app-a".to_string(),
    );
    let read = proxy.process_request(read);
    assert_eq!(
        read.body["data"]["native"],
        json!({"id": "gid://shopify/MarketingActivity/1001", "isExternal": false})
    );
    assert_eq!(
        read.body["data"]["parent"],
        json!({"id": parent_id, "remoteId": "delete-guard-parent"})
    );
    assert_eq!(read.body["data"]["external"], Value::Null);
}

#[test]
fn inventory_quantity_roots_stage_set_move_properties_and_downstream_reads() {
    let mut proxy = inventory_seed_proxy();
    let shop_location_id = add_inventory_test_location(&mut proxy, "Shop location");
    let custom_location_id = add_inventory_test_location(&mut proxy, "My Custom Location");

    let empty = proxy.process_request(json_graphql_request(
        r#"
        query InventoryItemsEmptyRead {
          inventoryItems(first: 1, query: "id:0") { nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        empty.body["data"]["inventoryItems"],
        json!({"nodes": [], "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null}})
    );

    let properties = proxy.process_request(json_graphql_request(
        r#"
        query InventoryPropertiesRead { inventoryProperties { quantityNames { name displayName isInUse belongsTo comprises } } }
        "#,
        json!({}),
    ));
    assert_eq!(
        properties.body["data"]["inventoryProperties"]["quantityNames"][0],
        json!({"name": "available", "displayName": "Available", "isInUse": true, "belongsTo": ["on_hand"], "comprises": []})
    );

    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryQuantityRootSetup($input: ProductSetInput!, $synchronous: Boolean!) {
          productSet(input: $input, synchronous: $synchronous) {
            product {
              id
              variants(first: 1) {
                nodes { inventoryItem { id } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "synchronous": true,
            "input": {
                "title": "Inventory quantity root setup",
                "status": "DRAFT",
                "productOptions": [{
                    "name": "Title",
                    "position": 1,
                    "values": [{ "name": "Default Title" }]
                }],
                "variants": [{
                    "optionValues": [{ "optionName": "Title", "name": "Default Title" }],
                    "inventoryItem": { "tracked": true, "requiresShipping": true },
                    "inventoryQuantities": [
                        { "locationId": shop_location_id, "name": "available", "quantity": 0 },
                        { "locationId": custom_location_id, "name": "available", "quantity": 0 }
                    ]
                }]
            }
        }),
    ));
    assert_eq!(setup.body["data"]["productSet"]["userErrors"], json!([]));
    let product_id = setup.body["data"]["productSet"]["product"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let inventory_item_id = setup.body["data"]["productSet"]["product"]["variants"]["nodes"][0]
        ["inventoryItem"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryQuantitySet($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            inventoryAdjustmentGroup { reason referenceDocumentUri changes { name delta quantityAfterChange ledgerDocumentUri location { id name } } }
            userErrors { field message }
          }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://har-305/set/1777251367654", "ignoreCompareQuantity": true, "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": shop_location_id, "quantity": 7},
            {"inventoryItemId": inventory_item_id, "locationId": custom_location_id, "quantity": 2}
        ]}}),
    ));
    assert_eq!(
        set.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );
    assert_eq!(
        set.body["data"]["inventorySetQuantities"]["inventoryAdjustmentGroup"]["changes"][0],
        json!({"name": "available", "delta": 7, "quantityAfterChange": null, "ledgerDocumentUri": null, "location": {"id": shop_location_id, "name": "Shop location"}})
    );
    assert_eq!(
        set.body["data"]["inventorySetQuantities"]["inventoryAdjustmentGroup"]["changes"][2]
            ["name"],
        json!("on_hand")
    );

    let read_after_set = proxy.process_request(json_graphql_request(
        r#"
        query InventoryQuantityDownstreamRead($inventoryItemId: ID!, $productId: ID!) {
          inventoryItem(id: $inventoryItemId) {
            variant { inventoryQuantity product { totalInventory } }
            inventoryLevels(first: 10) { nodes { location { id } quantities(names: ["available", "on_hand", "damaged"]) { name quantity updatedAt } } }
          }
          product(id: $productId) { totalInventory }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "productId": product_id}),
    ));
    assert_eq!(
        read_after_set.body["data"]["inventoryItem"]["variant"]["inventoryQuantity"],
        json!(9)
    );
    assert_eq!(
        read_after_set.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [0]["quantity"],
        json!(7)
    );
    assert_eq!(
        read_after_set.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [0]["updatedAt"],
        json!("2024-01-01T00:00:01.000Z")
    );
    assert_eq!(
        read_after_set.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [1]["updatedAt"],
        json!("2024-01-01T00:00:01.000Z")
    );
    assert_eq!(
        read_after_set.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [2]["updatedAt"],
        Value::Null
    );
    assert_eq!(
        read_after_set.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][1]["quantities"]
            [1]["quantity"],
        json!(2)
    );

    let move_response = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryQuantityMove($input: InventoryMoveQuantitiesInput!) {
          inventoryMoveQuantities(input: $input) {
            inventoryAdjustmentGroup { id createdAt reason referenceDocumentUri changes { name delta quantityAfterChange ledgerDocumentUri location { id name } } }
            userErrors { field message }
          }
        }
        "#,
        json!({"input": {"reason": "correction", "referenceDocumentUri": "logistics://har-305/move/1777251367654", "changes": [{"inventoryItemId": inventory_item_id, "quantity": 3, "from": {"locationId": shop_location_id, "name": "available"}, "to": {"locationId": shop_location_id, "name": "damaged", "ledgerDocumentUri": "ledger://har-305/move/to/1777251367654"}}]}}),
    ));
    assert_eq!(
        move_response.body["data"]["inventoryMoveQuantities"]["userErrors"],
        json!([])
    );
    assert_eq!(
        move_response.body["data"]["inventoryMoveQuantities"]["inventoryAdjustmentGroup"]
            ["changes"][0]["delta"],
        json!(-3)
    );
    assert!(
        move_response.body["data"]["inventoryMoveQuantities"]["inventoryAdjustmentGroup"]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("gid://shopify/InventoryAdjustmentGroup/"))
    );
    assert!(
        move_response.body["data"]["inventoryMoveQuantities"]["inventoryAdjustmentGroup"]
            ["createdAt"]
            .as_str()
            .is_some_and(|created_at| created_at.ends_with('Z'))
    );
    // Real Shopify reports quantityAfterChange as null for move adjustment-group
    // changes (confirmed in inventory-quantity-roots-parity cassette).
    assert_eq!(
        move_response.body["data"]["inventoryMoveQuantities"]["inventoryAdjustmentGroup"]
            ["changes"][0]["quantityAfterChange"],
        Value::Null
    );
    assert_eq!(
        move_response.body["data"]["inventoryMoveQuantities"]["inventoryAdjustmentGroup"]
            ["changes"][1]["delta"],
        json!(3)
    );
    assert_eq!(
        move_response.body["data"]["inventoryMoveQuantities"]["inventoryAdjustmentGroup"]
            ["changes"][1]["quantityAfterChange"],
        Value::Null
    );

    let read_after_move = proxy.process_request(json_graphql_request(
        r#"
        query InventoryQuantityDownstreamRead($inventoryItemId: ID!, $productId: ID!) {
          inventoryItem(id: $inventoryItemId) {
            variant { inventoryQuantity product { totalInventory } }
            inventoryLevels(first: 10) { nodes { location { id } quantities(names: ["available", "on_hand", "damaged"]) { name quantity updatedAt } } }
          }
          product(id: $productId) { totalInventory }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "productId": product_id}),
    ));
    assert_eq!(
        read_after_move.body["data"]["inventoryItem"]["variant"]["inventoryQuantity"],
        json!(6)
    );
    assert_eq!(
        read_after_move.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [0]["quantity"],
        json!(4)
    );
    assert_eq!(
        read_after_move.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [0]["updatedAt"],
        json!("2024-01-01T00:00:02.000Z")
    );
    assert_eq!(
        read_after_move.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [1]["updatedAt"],
        json!("2024-01-01T00:00:01.000Z")
    );
    assert_eq!(
        read_after_move.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [2]["quantity"],
        json!(3)
    );
    assert_eq!(
        read_after_move.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [2]["updatedAt"],
        json!("2024-01-01T00:00:02.000Z")
    );

    let blocked_set = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryQuantitySet($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            userErrors { field message }
          }
        }
        "#,
        json!({"idempotencyKey": "inventory-set-missing-change-from", "input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://har-305/set/blocked", "quantities": [{"inventoryItemId": inventory_item_id, "locationId": shop_location_id, "quantity": 7}]}}),
    ));
    assert_eq!(
        blocked_set.body["errors"][0]["message"],
        json!("InventoryQuantityInput must include the following argument: changeFromQuantity.")
    );
    assert_eq!(
        blocked_set.body["data"]["inventorySetQuantities"],
        Value::Null
    );

    let blocked_move = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryQuantityMove($input: InventoryMoveQuantitiesInput!) { inventoryMoveQuantities(input: $input) { userErrors { field message } } }
        "#,
        json!({"input": {"reason": "correction", "referenceDocumentUri": "logistics://har-305/move/blocked", "changes": [{"inventoryItemId": inventory_item_id, "quantity": 1, "from": {"locationId": shop_location_id, "name": "available"}, "to": {"locationId": custom_location_id, "name": "damaged", "ledgerDocumentUri": "ledger://har-305/move/blocked"}}]}}),
    ));
    assert_eq!(
        blocked_move.body["data"]["inventoryMoveQuantities"]["userErrors"],
        json!([{"field": ["input", "changes", "0"], "message": "The quantities can't be moved between different locations."}])
    );
}

#[test]
fn inventory_quantity_mutations_reject_non_sentinel_unknown_ids() {
    let mut proxy = inventory_seed_proxy();
    let (_variant_id, inventory_item_id) =
        create_inventory_test_item(&mut proxy, "STRICT-EXISTENCE");
    let location_id = add_inventory_test_location(&mut proxy, "Strict existence location");
    let unknown_item_id = "gid://shopify/InventoryItem/not-created-for-strict-check";
    let unknown_location_id = "gid://shopify/Location/not-created-for-strict-check";

    let unknown_item = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownInventoryItem($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "strict-unknown-item", "input": {"name": "available", "reason": "correction", "quantities": [
            {"inventoryItemId": unknown_item_id, "locationId": location_id, "quantity": 3, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        unknown_item.body["data"]["inventorySetQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "quantities", "0", "inventoryItemId"],
                "message": "The specified inventory item could not be found.",
                "code": "INVALID_INVENTORY_ITEM"
            }]
        })
    );

    let unknown_location = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownInventoryLocation($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "strict-unknown-location", "input": {"name": "available", "reason": "correction", "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": unknown_location_id, "quantity": 3, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        unknown_location.body["data"]["inventorySetQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "quantities", "0", "locationId"],
                "message": "The specified location could not be found.",
                "code": "INVALID_LOCATION"
            }]
        })
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["interpreted"]["operationName"] == json!("inventorySetQuantities"))
            .count(),
        0
    );
}

#[test]
fn inventory_items_connection_lists_staged_inventory_item() {
    let mut proxy = inventory_seed_proxy();
    let (_variant_id, inventory_item_id) = create_inventory_test_item(&mut proxy, "LIST-STAGED");
    let location_id = add_inventory_test_location(&mut proxy, "Connection Stockroom");

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedInventoryConnection($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "inventory-items-connection", "input": {"name": "available", "reason": "correction", "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": 6, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        set.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query InventoryItemsConnection($query: String!) {
          inventoryItems(first: 10, query: $query) {
            nodes {
              id
              tracked
              inventoryLevels(first: 5) {
                nodes {
                  location { id name }
                  quantities(names: ["available", "on_hand"]) { name quantity }
                }
              }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"query": "tracked:true"}),
    ));
    assert_eq!(
        read.body["data"]["inventoryItems"]["nodes"][0]["id"],
        json!(inventory_item_id)
    );
    assert_eq!(
        read.body["data"]["inventoryItems"]["nodes"][0]["inventoryLevels"]["nodes"][0]["location"],
        json!({"id": location_id, "name": "Connection Stockroom"})
    );
    assert_eq!(
        read.body["data"]["inventoryItems"]["nodes"][0]["inventoryLevels"]["nodes"][0]
            ["quantities"],
        json!([
            {"name": "available", "quantity": 6},
            {"name": "on_hand", "quantity": 6}
        ])
    );
}

#[test]
fn inventory_items_query_filters_window_and_rejects_unknown_tokens() {
    let mut proxy = inventory_seed_proxy();
    let (_first_variant_id, first_item_id) = create_inventory_test_item(&mut proxy, "FILTER-ALPHA");
    let (_second_variant_id, second_item_id) =
        create_inventory_test_item(&mut proxy, "FILTER-BETA");
    let second_item_tail = second_item_id
        .rsplit('/')
        .next()
        .unwrap()
        .split('?')
        .next()
        .unwrap();

    let mark_second_untracked = proxy.process_request(json_graphql_request(
        r#"
        mutation MarkInventoryItemUntracked($id: ID!, $input: InventoryItemInput!) {
          inventoryItemUpdate(id: $id, input: $input) {
            inventoryItem { id tracked }
            userErrors { field message }
          }
        }
        "#,
        json!({"id": second_item_id, "input": {"tracked": false}}),
    ));
    assert_eq!(
        mark_second_untracked.body["data"]["inventoryItemUpdate"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query InventoryItemsFiltered($after: String, $idRange: String!) {
          firstPage: inventoryItems(first: 1) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          secondPage: inventoryItems(first: 1, after: $after) {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          unknownFilter: inventoryItems(first: 10, query: "unknown_field:anything") {
            nodes { id }
          }
          invalidTracked: inventoryItems(first: 10, query: "tracked:notaboolean") {
            nodes { id }
          }
          skuQuoted: inventoryItems(first: 10, query: "sku:'FILTER-ALPHA'") {
            nodes { id }
          }
          idRange: inventoryItems(first: 10, query: $idRange) {
            nodes { id }
          }
          trackedFalse: inventoryItems(first: 10, query: "tracked:false") {
            nodes { id tracked }
          }
          updatedBefore: inventoryItems(first: 10, query: "updated_at:<2024-01-01T00:00:00.000Z") {
            nodes { id }
          }
        }
        "#,
        json!({
            "after": first_item_id,
            "idRange": format!("id:>={second_item_tail}")
        }),
    ));

    assert_eq!(
        read.body["data"]["firstPage"],
        json!({
            "nodes": [{"id": first_item_id}],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": first_item_id,
                "endCursor": first_item_id
            }
        })
    );
    assert_eq!(
        read.body["data"]["secondPage"]["nodes"],
        json!([{ "id": second_item_id }])
    );
    assert_eq!(
        read.body["data"]["secondPage"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );
    assert_eq!(read.body["data"]["unknownFilter"]["nodes"], json!([]));
    assert_eq!(read.body["data"]["invalidTracked"]["nodes"], json!([]));
    assert_eq!(
        read.body["data"]["skuQuoted"]["nodes"],
        json!([{ "id": first_item_id }])
    );
    assert_eq!(
        read.body["data"]["idRange"]["nodes"],
        json!([{ "id": second_item_id }])
    );
    assert_eq!(
        read.body["data"]["trackedFalse"]["nodes"],
        json!([{ "id": second_item_id, "tracked": false }])
    );
    assert_eq!(read.body["data"]["updatedBefore"]["nodes"], json!([]));
}

#[test]
fn order_create_inventory_decrement_uses_staged_default_location() {
    let mut proxy = inventory_seed_proxy();
    let (variant_id, inventory_item_id) = create_inventory_test_item(&mut proxy, "DEFAULT-LOC");
    let location_id = add_inventory_test_location(&mut proxy, "Primary Fulfillment");

    let order = proxy.process_request(json_graphql_request(
        r#"
        mutation OrderCreateInventoryDefaultLocation($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
          orderCreate(order: $order, options: $options) {
            order { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "inventory-default-location@example.com",
                "currency": "USD",
                "lineItems": [{
                    "variantId": variant_id,
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            },
            "options": {
                "sendReceipt": false,
                "sendFulfillmentReceipt": false
            }
        }),
    ));
    assert_eq!(order.body["data"]["orderCreate"]["userErrors"], json!([]));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query InventoryAfterDefaultLocation($id: ID!) {
          inventoryItem(id: $id) {
            inventoryLevels(first: 5) {
              nodes {
                location { id name }
                quantities(names: ["available", "on_hand"]) { name quantity }
              }
            }
          }
        }
        "#,
        json!({"id": inventory_item_id}),
    ));
    assert_eq!(
        read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["location"],
        json!({"id": location_id, "name": "Primary Fulfillment"})
    );
    assert_eq!(
        read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"],
        json!([
            {"name": "available", "quantity": -2},
            {"name": "on_hand", "quantity": 0}
        ])
    );
}

#[test]
fn inventory_adjust_quantities_stages_levels_logs_and_reads_back_by_root_field() {
    let mut proxy = inventory_seed_proxy();
    let (_variant_id, inventory_item_id) = create_inventory_test_item(&mut proxy, "ADJUST-ROOT");
    let location_id = add_inventory_test_location(&mut proxy, "Source location");

    let adjust = proxy.process_request(json_graphql_request(
        r#"
        mutation AnyOperationName($input: InventoryAdjustQuantitiesInput!) {
          adjust: inventoryAdjustQuantities(input: $input) {
            inventoryAdjustmentGroup {
              reason
              referenceDocumentUri
              changes {
                name
                delta
                item { id }
                location { id name }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://inventory/adjust", "changes": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "delta": 5, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        adjust.body["data"]["adjust"]["inventoryAdjustmentGroup"]["changes"][0],
        json!({"name": "available", "delta": 5, "item": {"id": inventory_item_id}, "location": {"id": location_id, "name": "Source location"}})
    );
    assert_eq!(adjust.body["data"]["adjust"]["userErrors"], json!([]));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query StoreBackedInventoryRead($id: ID!) {
          inventoryItem(id: $id) {
            id
            tracked
            variant { inventoryQuantity }
            inventoryLevels(first: 5) {
              nodes {
                id
                item { id }
                location { id name }
                quantities(names: ["available", "on_hand", "damaged"]) { name quantity updatedAt }
              }
            }
          }
        }
        "#,
        json!({"id": inventory_item_id}),
    ));
    assert_eq!(
        read.body["data"]["inventoryItem"]["variant"]["inventoryQuantity"],
        json!(5)
    );
    assert_eq!(
        read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"],
        json!([
            {"name": "available", "quantity": 5, "updatedAt": "2024-01-01T00:00:00.000Z"},
            {"name": "on_hand", "quantity": 5, "updatedAt": "2024-01-01T00:00:00.000Z"},
            {"name": "damaged", "quantity": 0, "updatedAt": null}
        ])
    );
    assert_eq!(
        read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["item"],
        json!({"id": inventory_item_id})
    );

    let level_id = read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let level_read = proxy.process_request(json_graphql_request(
        r#"
        query StoreBackedInventoryLevelRead($id: ID!) {
          inventoryLevel(id: $id) {
            location { id }
            quantities(names: ["available", "on_hand"]) { name quantity }
          }
        }
        "#,
        json!({"id": level_id}),
    ));
    assert_eq!(
        level_read.body["data"]["inventoryLevel"]["quantities"],
        json!([
            {"name": "available", "quantity": 5},
            {"name": "on_hand", "quantity": 5}
        ])
    );

    let invalid_reason = proxy.process_request(json_graphql_request(
        r#"
        mutation StoreBackedInventoryInvalidReason($input: InventoryAdjustQuantitiesInput!) {
          inventoryAdjustQuantities(input: $input) {
            inventoryAdjustmentGroup { reason }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"name": "available", "reason": "not_a_reason", "changes": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "delta": 1, "changeFromQuantity": 5}
        ]}}),
    ));
    assert_eq!(
        invalid_reason.body["data"]["inventoryAdjustQuantities"]["userErrors"][0]["code"],
        json!("INVALID_REASON")
    );

    let log = log_snapshot(&proxy);
    let adjust_log = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["interpreted"]["operationName"] == json!("inventoryAdjustQuantities"))
        .expect("inventoryAdjustQuantities should be logged");
    assert_eq!(adjust_log["status"], json!("staged"));
}

#[test]
fn inventory_adjust_quantities_all_zero_delta_is_unlogged_noop() {
    let mut proxy = inventory_seed_proxy();
    let (_variant_id, inventory_item_id) = create_inventory_test_item(&mut proxy, "ZERO-DELTA");
    let location_id = add_inventory_test_location(&mut proxy, "Zero delta location");

    let adjust = proxy.process_request(json_graphql_request(
        r#"
        mutation ZeroDeltaAdjust($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
          inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup {
              id
              reason
              changes { name delta quantityAfterChange item { id } location { id name } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "inventory-adjust-zero-delta-noop", "input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://inventory/adjust/zero", "changes": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "delta": 0, "changeFromQuantity": 0}
        ]}}),
    ));

    assert_eq!(
        adjust.body["data"]["inventoryAdjustQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": []
        })
    );
    assert_no_inventory_quantity_logs(&proxy);
    assert!(state_snapshot(&proxy)["stagedState"]["inventoryLevels"].is_null());
}

#[test]
fn inventory_adjust_quantities_mixed_zero_and_nonzero_delta_stages_nonzero_change() {
    let mut proxy = inventory_seed_proxy();
    let (_zero_variant_id, zero_item_id) = create_inventory_test_item(&mut proxy, "MIXED-ZERO");
    let (_nonzero_variant_id, nonzero_item_id) =
        create_inventory_test_item(&mut proxy, "MIXED-NONZERO");
    let location_id = add_inventory_test_location(&mut proxy, "Mixed delta location");

    let adjust = proxy.process_request(json_graphql_request(
        r#"
        mutation MixedDeltaAdjust($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
          inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup {
              id
              reason
              changes { name delta item { id } location { id name } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "inventory-adjust-mixed-delta", "input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://inventory/adjust/mixed", "changes": [
            {"inventoryItemId": zero_item_id, "locationId": location_id, "delta": 0, "changeFromQuantity": 0},
            {"inventoryItemId": nonzero_item_id, "locationId": location_id, "delta": 3, "changeFromQuantity": 0}
        ]}}),
    ));

    let payload = &adjust.body["data"]["inventoryAdjustQuantities"];
    assert_eq!(payload["userErrors"], json!([]));
    assert!(payload["inventoryAdjustmentGroup"]["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("gid://shopify/InventoryAdjustmentGroup/")));
    let changes = payload["inventoryAdjustmentGroup"]["changes"]
        .as_array()
        .expect("mixed adjust should return change rows");
    assert!(changes.iter().all(|change| change["delta"] != json!(0)));
    assert!(changes.iter().any(|change| {
        change["name"] == json!("available")
            && change["delta"] == json!(3)
            && change["item"]["id"] == json!(nonzero_item_id)
    }));
    assert!(changes.iter().any(|change| {
        change["name"] == json!("on_hand")
            && change["delta"] == json!(3)
            && change["item"]["id"] == json!(nonzero_item_id)
    }));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MixedDeltaInventoryRead($id: ID!) {
          inventoryItem(id: $id) {
            inventoryLevels(first: 5) {
              nodes {
                quantities(names: ["available", "on_hand"]) { name quantity }
              }
            }
          }
        }
        "#,
        json!({"id": nonzero_item_id}),
    ));
    assert_eq!(
        read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"],
        json!([
            {"name": "available", "quantity": 3},
            {"name": "on_hand", "quantity": 3}
        ])
    );

    let log = log_snapshot(&proxy);
    let adjust_log = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["interpreted"]["operationName"] == json!("inventoryAdjustQuantities"))
        .expect("inventoryAdjustQuantities should be logged");
    assert_eq!(adjust_log["status"], json!("staged"));
}

#[test]
fn inventory_adjust_quantities_mirrors_on_hand_for_captured_non_available_names() {
    let mut proxy = snapshot_proxy();

    let setup = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryAdjustMirrorSetup($input: ProductSetInput!, $synchronous: Boolean!) {
          productSet(input: $input, synchronous: $synchronous) {
            product {
              id
              totalInventory
              tracksInventory
              variants(first: 1) {
                nodes {
                  id
                  inventoryQuantity
                  inventoryItem { id }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "synchronous": true,
            "input": {
                "title": "Inventory adjust on-hand mirror runtime seed",
                "status": "DRAFT",
                "productOptions": [{
                    "name": "Title",
                    "position": 1,
                    "values": [{ "name": "Default Title" }]
                }],
                "variants": [{
                    "optionValues": [{ "optionName": "Title", "name": "Default Title" }],
                    "inventoryItem": { "tracked": true, "requiresShipping": true },
                    "inventoryQuantities": [{
                        "locationId": "gid://shopify/Location/1",
                        "name": "available",
                        "quantity": 0
                    }]
                }]
            }
        }),
    ));
    assert_eq!(setup.body["data"]["productSet"]["userErrors"], json!([]));
    let product = &setup.body["data"]["productSet"]["product"];
    let product_id = product["id"].as_str().unwrap().to_string();
    let variant_id = product["variants"]["nodes"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let inventory_item_id = product["variants"]["nodes"][0]["inventoryItem"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut expected_damaged = 0;
    let mut expected_quality_control = 0;
    let mut expected_reserved = 0;
    let mut expected_safety_stock = 0;
    let mut expected_incoming = 0;
    let mut expected_on_hand = 0;

    for (name, reason, delta) in [
        ("damaged", "damaged", 2),
        ("reserved", "reservation_created", 3),
        ("quality_control", "quality_control", 4),
        ("safety_stock", "safety_stock", 5),
    ] {
        let ledger = format!("https://example.com/inventory-adjust-mirror/{name}");
        let adjust = proxy.process_request(json_graphql_request(
            r#"
            mutation InventoryAdjustMirror($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
              inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
                inventoryAdjustmentGroup {
                  reason
                  changes { name delta quantityAfterChange ledgerDocumentUri item { id } location { id } }
                }
                userErrors { field message code }
              }
            }
            "#,
            json!({"idempotencyKey": format!("inventory-adjust-mirror-{name}"), "input": {
                "name": name,
                "reason": reason,
                "changes": [{
                    "inventoryItemId": inventory_item_id,
                    "locationId": "gid://shopify/Location/1",
                    "delta": delta,
                    "changeFromQuantity": 0,
                    "ledgerDocumentUri": ledger
                }]
            }}),
        ));
        let payload = &adjust.body["data"]["inventoryAdjustQuantities"];
        assert_eq!(payload["userErrors"], json!([]));
        assert_eq!(payload["inventoryAdjustmentGroup"]["reason"], json!(reason));
        assert_eq!(
            payload["inventoryAdjustmentGroup"]["changes"],
            json!([
                {
                    "name": name,
                    "delta": delta,
                    "quantityAfterChange": null,
                    "ledgerDocumentUri": ledger,
                    "item": { "id": inventory_item_id },
                    "location": { "id": "gid://shopify/Location/1" }
                },
                {
                    "name": "on_hand",
                    "delta": delta,
                    "quantityAfterChange": null,
                    "ledgerDocumentUri": null,
                    "item": { "id": inventory_item_id },
                    "location": { "id": "gid://shopify/Location/1" }
                }
            ])
        );

        match name {
            "damaged" => expected_damaged += delta,
            "reserved" => expected_reserved += delta,
            "quality_control" => expected_quality_control += delta,
            "safety_stock" => expected_safety_stock += delta,
            _ => unreachable!(),
        }
        expected_on_hand += delta;

        let read = proxy.process_request(json_graphql_request(
            r#"
            query InventoryAdjustMirrorRead($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) {
              product(id: $productId) { totalInventory tracksInventory }
              productVariant(id: $variantId) {
                inventoryQuantity
                inventoryItem {
                  inventoryLevels(first: 5) {
                    nodes {
                      quantities(names: ["available", "incoming", "damaged", "quality_control", "reserved", "safety_stock", "on_hand"]) {
                        name
                        quantity
                        updatedAt
                      }
                    }
                  }
                }
              }
              inventoryItem(id: $inventoryItemId) {
                variant { inventoryQuantity product { totalInventory tracksInventory } }
                inventoryLevels(first: 5) {
                  nodes {
                    quantities(names: ["available", "incoming", "damaged", "quality_control", "reserved", "safety_stock", "on_hand"]) {
                      name
                      quantity
                      updatedAt
                    }
                  }
                }
              }
            }
            "#,
            json!({
                "productId": product_id,
                "variantId": variant_id,
                "inventoryItemId": inventory_item_id
            }),
        ));
        assert_eq!(read.body["data"]["product"]["totalInventory"], json!(0));
        assert_eq!(read.body["data"]["product"]["tracksInventory"], json!(true));
        assert_eq!(
            read.body["data"]["productVariant"]["inventoryQuantity"],
            json!(0)
        );
        assert_eq!(
            read.body["data"]["inventoryItem"]["variant"]["inventoryQuantity"],
            json!(0)
        );
        assert_eq!(
            read.body["data"]["inventoryItem"]["variant"]["product"],
            json!({"totalInventory": 0, "tracksInventory": true})
        );
        assert_eq!(
            read.body["data"]["productVariant"]["inventoryItem"]["inventoryLevels"]["nodes"][0]
                ["quantities"],
            read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
        );
        let rows = &read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"];
        let quantity = |name: &str| {
            rows.as_array()
                .unwrap()
                .iter()
                .find(|row| row["name"] == json!(name))
                .and_then(|row| row["quantity"].as_i64())
                .unwrap()
        };
        let updated_at = |name: &str| {
            rows.as_array()
                .unwrap()
                .iter()
                .find(|row| row["name"] == json!(name))
                .map(|row| row["updatedAt"].clone())
                .unwrap()
        };
        assert_eq!(quantity("available"), 0);
        assert_eq!(quantity("incoming"), expected_incoming);
        assert_eq!(quantity("damaged"), expected_damaged);
        assert_eq!(quantity("quality_control"), expected_quality_control);
        assert_eq!(quantity("reserved"), expected_reserved);
        assert_eq!(quantity("safety_stock"), expected_safety_stock);
        assert_eq!(quantity("on_hand"), expected_on_hand);
        assert_eq!(updated_at("on_hand"), Value::Null);
    }

    let incoming_ledger = "https://example.com/inventory-adjust-mirror/incoming";
    let incoming = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryAdjustIncomingControl($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
          inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup {
              reason
              changes { name delta quantityAfterChange ledgerDocumentUri item { id } location { id } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "inventory-adjust-mirror-incoming-control", "input": {
            "name": "incoming",
            "reason": "received",
            "changes": [{
                "inventoryItemId": inventory_item_id,
                "locationId": "gid://shopify/Location/1",
                "delta": 6,
                "changeFromQuantity": 0,
                "ledgerDocumentUri": incoming_ledger
            }]
        }}),
    ));
    assert_eq!(
        incoming.body["data"]["inventoryAdjustQuantities"]["inventoryAdjustmentGroup"]["changes"],
        json!([{
            "name": "incoming",
            "delta": 6,
            "quantityAfterChange": null,
            "ledgerDocumentUri": incoming_ledger,
            "item": { "id": inventory_item_id },
            "location": { "id": "gid://shopify/Location/1" }
        }])
    );
    assert_eq!(
        incoming.body["data"]["inventoryAdjustQuantities"]["userErrors"],
        json!([])
    );
    expected_incoming += 6;

    let read = proxy.process_request(json_graphql_request(
        r#"
        query InventoryAdjustIncomingControlRead($productId: ID!, $variantId: ID!, $inventoryItemId: ID!) {
          product(id: $productId) { totalInventory tracksInventory }
          productVariant(id: $variantId) { inventoryQuantity }
          inventoryItem(id: $inventoryItemId) {
            variant { inventoryQuantity product { totalInventory tracksInventory } }
            inventoryLevels(first: 5) {
              nodes {
                quantities(names: ["available", "incoming", "damaged", "quality_control", "reserved", "safety_stock", "on_hand"]) {
                  name
                  quantity
                  updatedAt
                }
              }
            }
          }
        }
        "#,
        json!({
            "productId": product_id,
            "variantId": variant_id,
            "inventoryItemId": inventory_item_id
        }),
    ));
    let rows = &read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"];
    let quantity = |name: &str| {
        rows.as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == json!(name))
            .and_then(|row| row["quantity"].as_i64())
            .unwrap()
    };
    assert_eq!(
        read.body["data"]["product"],
        json!({"totalInventory": 0, "tracksInventory": true})
    );
    assert_eq!(
        read.body["data"]["productVariant"]["inventoryQuantity"],
        json!(0)
    );
    assert_eq!(
        read.body["data"]["inventoryItem"]["variant"],
        json!({"inventoryQuantity": 0, "product": {"totalInventory": 0, "tracksInventory": true}})
    );
    assert_eq!(quantity("available"), 0);
    assert_eq!(quantity("incoming"), expected_incoming);
    assert_eq!(quantity("damaged"), expected_damaged);
    assert_eq!(quantity("quality_control"), expected_quality_control);
    assert_eq!(quantity("reserved"), expected_reserved);
    assert_eq!(quantity("safety_stock"), expected_safety_stock);
    assert_eq!(quantity("on_hand"), expected_on_hand);
}

#[test]
fn inventory_quantity_mutations_reject_unknown_inventory_item_without_staging() {
    let mut proxy = inventory_seed_proxy();
    let unknown_inventory_item_id = "gid://shopify/InventoryItem/424242424242";
    let location_id = add_inventory_test_location(&mut proxy, "Known location");

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownItemSet($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "unknown-item-set", "input": {"name": "available", "reason": "correction", "quantities": [
            {"inventoryItemId": unknown_inventory_item_id, "locationId": location_id, "quantity": 3, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        set.body["data"]["inventorySetQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "quantities", "0", "inventoryItemId"],
                "message": "The specified inventory item could not be found.",
                "code": "INVALID_INVENTORY_ITEM"
            }]
        })
    );

    let adjust = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownItemAdjust($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
          inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "unknown-item-adjust", "input": {"name": "available", "reason": "correction", "changes": [
            {"inventoryItemId": unknown_inventory_item_id, "locationId": location_id, "delta": 1, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        adjust.body["data"]["inventoryAdjustQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "changes", "0", "inventoryItemId"],
                "message": "The specified inventory item could not be found.",
                "code": "INVALID_INVENTORY_ITEM"
            }]
        })
    );

    let move_response = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownItemMove($input: InventoryMoveQuantitiesInput!, $idempotencyKey: String!) {
          inventoryMoveQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "unknown-item-move", "input": {"reason": "correction", "changes": [{
            "inventoryItemId": unknown_inventory_item_id,
            "quantity": 1,
            "from": {"locationId": location_id, "name": "available", "changeFromQuantity": 0},
            "to": {"locationId": location_id, "name": "damaged", "changeFromQuantity": 0, "ledgerDocumentUri": "ledger://inventory/unknown-item"}
        }]}}),
    ));
    assert_eq!(
        move_response.body["data"]["inventoryMoveQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "changes", "0", "inventoryItemId"],
                "message": "The specified inventory item could not be found.",
                "code": "INVALID_INVENTORY_ITEM"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query UnknownInventoryItemRead($id: ID!) {
          inventoryItem(id: $id) { id inventoryLevels(first: 5) { nodes { location { id } } } }
        }
        "#,
        json!({"id": unknown_inventory_item_id}),
    ));
    assert_eq!(read.body["data"]["inventoryItem"], Value::Null);
    assert_no_inventory_quantity_logs(&proxy);
}

#[test]
fn inventory_quantity_mutations_reject_unknown_location_without_staging() {
    let mut proxy = inventory_seed_proxy();
    let (_variant_id, inventory_item_id) =
        create_inventory_test_item(&mut proxy, "UNKNOWN-LOCATION");
    let unknown_location_id = "gid://shopify/Location/515151515151";

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownLocationSet($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "unknown-location-set", "input": {"name": "available", "reason": "correction", "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": unknown_location_id, "quantity": 3, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        set.body["data"]["inventorySetQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "quantities", "0", "locationId"],
                "message": "The specified location could not be found.",
                "code": "INVALID_LOCATION"
            }]
        })
    );

    let adjust = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownLocationAdjust($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
          inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "unknown-location-adjust", "input": {"name": "available", "reason": "correction", "changes": [
            {"inventoryItemId": inventory_item_id, "locationId": unknown_location_id, "delta": 1, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        adjust.body["data"]["inventoryAdjustQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "changes", "0", "locationId"],
                "message": "The specified location could not be found.",
                "code": "INVALID_LOCATION"
            }]
        })
    );

    let move_response = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownLocationMove($input: InventoryMoveQuantitiesInput!, $idempotencyKey: String!) {
          inventoryMoveQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "unknown-location-move", "input": {"reason": "correction", "changes": [{
            "inventoryItemId": inventory_item_id,
            "quantity": 1,
            "from": {"locationId": unknown_location_id, "name": "available", "changeFromQuantity": 0},
            "to": {"locationId": unknown_location_id, "name": "damaged", "changeFromQuantity": 0, "ledgerDocumentUri": "ledger://inventory/unknown-location"}
        }]}}),
    ));
    assert_eq!(
        move_response.body["data"]["inventoryMoveQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [
                {
                    "field": ["input", "changes", "0", "from", "locationId"],
                    "message": "The specified location could not be found.",
                    "code": "INVALID_LOCATION"
                },
                {
                    "field": ["input", "changes", "0", "to", "locationId"],
                    "message": "The specified location could not be found.",
                    "code": "INVALID_LOCATION"
                }
            ]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query UnknownLocationRead($id: ID!) {
          inventoryItem(id: $id) {
            id
            inventoryLevels(first: 5) { nodes { location { id } } }
          }
        }
        "#,
        json!({"id": inventory_item_id}),
    ));
    let levels = read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"]
        .as_array()
        .unwrap();
    assert!(!levels
        .iter()
        .any(|level| level["location"]["id"] == json!(unknown_location_id)));
    assert_no_inventory_quantity_logs(&proxy);
}

#[test]
fn inventory_level_reads_preserve_fulfillment_service_location_names() {
    let mut proxy = inventory_seed_proxy();
    let (_variant_id, inventory_item_id) =
        create_inventory_test_item(&mut proxy, "FS-LOCATION-NAME");

    let service = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateInventoryFulfillmentService($name: String!) {
          fulfillmentServiceCreate(name: $name, inventoryManagement: true) {
            fulfillmentService { location { id name } }
            userErrors { field message }
          }
        }
        "#,
        json!({"name": "Named Fulfillment Stockroom"}),
    ));
    assert_eq!(
        service.body["data"]["fulfillmentServiceCreate"]["userErrors"],
        json!([])
    );
    let location =
        &service.body["data"]["fulfillmentServiceCreate"]["fulfillmentService"]["location"];
    let location_id = location["id"].as_str().unwrap().to_string();
    assert_eq!(location["name"], json!("Named Fulfillment Stockroom"));

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation SetAtFulfillmentLocation($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            inventoryAdjustmentGroup { changes { name location { id name } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "ignoreCompareQuantity": true, "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": 4}
        ]}}),
    ));
    assert_eq!(
        set.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );
    assert_eq!(
        set.body["data"]["inventorySetQuantities"]["inventoryAdjustmentGroup"]["changes"][0]
            ["location"],
        json!({"id": location_id, "name": "Named Fulfillment Stockroom"})
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query FulfillmentLocationInventoryLevelName($inventoryItemId: ID!) {
          inventoryItem(id: $inventoryItemId) {
            inventoryLevels(first: 5) {
              nodes {
                location { id name }
                quantities(names: ["available", "on_hand"]) { name quantity }
              }
            }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id}),
    ));
    let level = read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|level| level["location"]["id"] == json!(location_id))
        .expect("inventory level at fulfillment service location should be readable");
    assert_eq!(
        level["location"],
        json!({"id": location_id, "name": "Named Fulfillment Stockroom"})
    );
    assert_eq!(
        level["quantities"],
        json!([
            {"name": "available", "quantity": 4},
            {"name": "on_hand", "quantity": 4}
        ])
    );
}

#[test]
fn inventory_set_on_hand_quantities_stages_locally_logs_and_reads_back() {
    use shopify_draft_proxy::proxy::UnsupportedMutationMode;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let calls = Arc::new(AtomicUsize::new(0));
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_base_products(vec![inventory_activation_base_product()])
    .with_upstream_transport({
        let calls = calls.clone();
        move |_request| {
            calls.fetch_add(1, Ordering::SeqCst);
            shopify_draft_proxy::proxy::Response {
                status: 599,
                headers: Default::default(),
                body: json!({"unexpectedUpstream": true}),
            }
        }
    });

    let variant = create_legacy_variant(
        &mut proxy,
        "gid://shopify/Product/1",
        "SET-ON-HAND",
        "10.00",
    );
    let variant_id = variant["id"].as_str().unwrap().to_string();
    let inventory_item_id = variant["inventoryItem"]["id"].as_str().unwrap().to_string();
    let location_id = add_inventory_test_location(&mut proxy, "Source location");
    calls.store(0, Ordering::SeqCst);

    let seed = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedInventory($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "ignoreCompareQuantity": true, "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": 2}
        ]}}),
    ));
    assert_eq!(
        seed.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let set_on_hand = proxy.process_request(json_graphql_request(
        r#"
        mutation SetOnHand($input: InventorySetOnHandQuantitiesInput!, $idempotencyKey: String!) {
          setOnHand: inventorySetOnHandQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup {
              id
              createdAt
              reason
              referenceDocumentUri
              changes {
                name
                delta
                quantityAfterChange
                item { id }
                location { id name }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "set-on-hand-local-staging", "input": {"reason": "correction", "referenceDocumentUri": "logistics://inventory/set-on-hand", "setQuantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": 10, "changeFromQuantity": 2}
        ]}}),
    ));
    assert_eq!(set_on_hand.status, 200);
    assert_eq!(
        set_on_hand.body["data"]["setOnHand"]["userErrors"],
        json!([])
    );
    let adjustment_group = &set_on_hand.body["data"]["setOnHand"]["inventoryAdjustmentGroup"];
    assert!(adjustment_group["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("gid://shopify/InventoryAdjustmentGroup/")));
    assert_eq!(
        adjustment_group["createdAt"],
        json!("2024-01-01T00:00:01.000Z")
    );
    assert_eq!(adjustment_group["reason"], json!("correction"));
    assert_eq!(
        adjustment_group["referenceDocumentUri"],
        json!("logistics://inventory/set-on-hand")
    );
    assert_eq!(
        adjustment_group["changes"],
        json!([
            {
                "name": "available",
                "delta": 8,
                "quantityAfterChange": null,
                "item": { "id": inventory_item_id },
                "location": { "id": location_id, "name": "Source location" }
            },
            {
                "name": "on_hand",
                "delta": 8,
                "quantityAfterChange": null,
                "item": { "id": inventory_item_id },
                "location": { "id": location_id, "name": "Source location" }
            }
        ])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query SetOnHandRead($inventoryItemId: ID!, $productId: ID!) {
          inventoryItem(id: $inventoryItemId) {
            variant { id inventoryQuantity product { id totalInventory tracksInventory } }
            inventoryLevels(first: 5) {
              nodes {
                quantities(names: ["available", "on_hand", "damaged"]) { name quantity updatedAt }
              }
            }
          }
          product(id: $productId) { totalInventory }
        }
        "#,
        json!({
            "inventoryItemId": inventory_item_id,
            "productId": "gid://shopify/Product/1"
        }),
    ));
    assert_eq!(
        read.body["data"]["inventoryItem"]["variant"]["inventoryQuantity"],
        json!(10)
    );
    assert_eq!(
        read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"],
        json!([
            {"name": "available", "quantity": 10, "updatedAt": "2024-01-01T00:00:01.000Z"},
            {"name": "on_hand", "quantity": 10, "updatedAt": null},
            {"name": "damaged", "quantity": 0, "updatedAt": null}
        ])
    );

    let product_read = proxy.process_request(json_graphql_request(
        r#"
        query SetOnHandProductRead($variantId: ID!, $productId: ID!) {
          productVariant(id: $variantId) { inventoryQuantity }
          product(id: $productId) {
            totalInventory
            hasOutOfStockVariants
            variants(first: 5) { nodes { inventoryQuantity inventoryItem { tracked } } }
          }
        }
        "#,
        json!({
            "variantId": variant_id,
            "productId": "gid://shopify/Product/1"
        }),
    ));
    assert_eq!(
        product_read.body["data"]["productVariant"]["inventoryQuantity"],
        json!(10)
    );
    assert_eq!(
        product_read.body["data"]["product"]["totalInventory"],
        json!(10)
    );
    assert_eq!(
        product_read.body["data"]["product"]["hasOutOfStockVariants"],
        json!(false)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let log = log_snapshot(&proxy);
    let log_entries = log["entries"].as_array().unwrap();
    let set_on_hand_log = log_entries
        .iter()
        .find(|entry| {
            entry["interpreted"]["operationName"] == json!("inventorySetOnHandQuantities")
        })
        .expect("inventorySetOnHandQuantities should be logged for commit replay");
    assert_eq!(set_on_hand_log["status"], json!("staged"));
    assert_eq!(
        set_on_hand_log["query"],
        json!(
            r#"
        mutation SetOnHand($input: InventorySetOnHandQuantitiesInput!, $idempotencyKey: String!) {
          setOnHand: inventorySetOnHandQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup {
              id
              createdAt
              reason
              referenceDocumentUri
              changes {
                name
                delta
                quantityAfterChange
                item { id }
                location { id name }
              }
            }
            userErrors { field message code }
          }
        }
        "#
        )
    );
    assert!(set_on_hand_log["rawBody"]
        .as_str()
        .is_some_and(|body| body.contains("inventorySetOnHandQuantities")));
}

#[test]
fn inventory_set_on_hand_quantities_validation_errors_are_local() {
    let mut proxy = inventory_seed_proxy();
    let (_variant_id, inventory_item_id) =
        create_inventory_test_item(&mut proxy, "SET-ON-HAND-VALIDATION");
    let location_id = add_inventory_test_location(&mut proxy, "Source location");

    let missing_idempotent = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingSetOnHandIdempotency($input: InventorySetOnHandQuantitiesInput!) {
          inventorySetOnHandQuantities(input: $input) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"reason": "correction", "setQuantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": 1, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        missing_idempotent.body["errors"][0]["message"],
        json!("The @idempotent directive is required for this mutation but was not provided.")
    );
    assert_eq!(
        missing_idempotent.body["errors"][0]["extensions"]["code"],
        json!("BAD_REQUEST")
    );
    assert_eq!(
        missing_idempotent.body["data"]["inventorySetOnHandQuantities"],
        Value::Null
    );

    let missing_change_from = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingSetOnHandChangeFrom($input: InventorySetOnHandQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetOnHandQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "set-on-hand-missing-change-from", "input": {"reason": "correction", "setQuantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": 1}
        ]}}),
    ));
    assert_eq!(
        missing_change_from.body["errors"][0]["message"],
        json!("InventorySetQuantityInput must include the following argument: changeFromQuantity.")
    );
    assert_eq!(
        missing_change_from.body["errors"][0]["extensions"]["code"],
        json!("INVALID_FIELD_ARGUMENTS")
    );
    assert_eq!(
        missing_change_from.body["data"]["inventorySetOnHandQuantities"],
        Value::Null
    );

    let unknown_item = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownItemSetOnHand($input: InventorySetOnHandQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetOnHandQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "set-on-hand-unknown-item", "input": {"reason": "correction", "setQuantities": [
            {"inventoryItemId": "gid://shopify/InventoryItem/626262626262", "locationId": location_id, "quantity": 1, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        unknown_item.body["data"]["inventorySetOnHandQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "setQuantities", "0", "inventoryItemId"],
                "message": "The specified inventory item could not be found.",
                "code": "INVALID_INVENTORY_ITEM"
            }]
        })
    );

    let unknown_location = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownLocationSetOnHand($input: InventorySetOnHandQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetOnHandQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "set-on-hand-unknown-location", "input": {"reason": "correction", "setQuantities": [
            {"inventoryItemId": inventory_item_id, "locationId": "gid://shopify/Location/737373737373", "quantity": 1, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        unknown_location.body["data"]["inventorySetOnHandQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "setQuantities", "0", "locationId"],
                "message": "The specified location could not be found.",
                "code": "INVALID_LOCATION"
            }]
        })
    );

    let negative_quantity = proxy.process_request(json_graphql_request(
        r#"
        mutation NegativeQuantitySetOnHand($input: InventorySetOnHandQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetOnHandQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "set-on-hand-negative", "input": {"reason": "correction", "setQuantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": -1, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        negative_quantity.body["data"]["inventorySetOnHandQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "setQuantities", "0", "quantity"],
                "message": "The quantity can't be negative.",
                "code": "INVALID_QUANTITY_NEGATIVE"
            }]
        })
    );

    let too_high_quantity = proxy.process_request(json_graphql_request(
        r#"
        mutation TooHighQuantitySetOnHand($input: InventorySetOnHandQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetOnHandQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "set-on-hand-too-high", "input": {"reason": "correction", "setQuantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": 1000000001, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        too_high_quantity.body["data"]["inventorySetOnHandQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "setQuantities", "0", "quantity"],
                "message": "The quantity can't be higher than 1,000,000,000.",
                "code": "INVALID_QUANTITY_TOO_HIGH"
            }]
        })
    );

    let invalid_reason = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidReasonSetOnHand($input: InventorySetOnHandQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetOnHandQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "set-on-hand-invalid-reason", "input": {"reason": "not_a_reason", "setQuantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": 1, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        invalid_reason.body["data"]["inventorySetOnHandQuantities"]["userErrors"][0]["code"],
        json!("INVALID_REASON")
    );
    assert_no_inventory_quantity_logs(&proxy);
}

#[test]
fn inventory_adjust_quantities_leaves_product_total_inventory_lazy() {
    let mut proxy = snapshot_proxy().with_base_products(vec![inventory_activation_base_product()]);
    let variant = create_legacy_variant(
        &mut proxy,
        "gid://shopify/Product/1",
        "ADJUST-LAZY",
        "10.00",
    );
    let variant_id = variant["id"].as_str().unwrap().to_string();
    let inventory_item_id = variant["inventoryItem"]["id"].as_str().unwrap().to_string();
    let location_id = add_inventory_test_location(&mut proxy, "Adjust lazy location");

    let seed = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedInventory($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) { userErrors { field message code } }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "ignoreCompareQuantity": true, "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": 2}
        ]}}),
    ));
    assert_eq!(
        seed.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let adjust = proxy.process_request(json_graphql_request(
        r#"
        mutation AdjustInventory($input: InventoryAdjustQuantitiesInput!) {
          inventoryAdjustQuantities(input: $input) {
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "changes": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "delta": -2, "changeFromQuantity": 2}
        ]}}),
    ));
    assert_eq!(
        adjust.body["data"]["inventoryAdjustQuantities"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query InventoryAdjustLazyProductAggregate($variantId: ID!, $productId: ID!) {
          productVariant(id: $variantId) { inventoryQuantity }
          product(id: $productId) {
            totalInventory
            hasOutOfStockVariants
            variants(first: 5) { nodes { inventoryQuantity inventoryItem { tracked } } }
          }
        }
        "#,
        json!({
            "variantId": variant_id,
            "productId": "gid://shopify/Product/1"
        }),
    ));
    assert_eq!(
        read.body["data"]["productVariant"]["inventoryQuantity"],
        json!(0)
    );
    assert_eq!(read.body["data"]["product"]["totalInventory"], json!(2));
    assert_eq!(
        read.body["data"]["product"]["hasOutOfStockVariants"],
        json!(true)
    );
    assert_eq!(
        read.body["data"]["product"]["variants"]["nodes"][0],
        json!({"inventoryQuantity": 0, "inventoryItem": {"tracked": true}})
    );
}

fn inventory_activation_base_product() -> ProductRecord {
    ProductRecord {
        id: "gid://shopify/Product/1".to_string(),
        created_at: "2024-01-01T00:00:00.000Z".to_string(),
        updated_at: "2024-01-01T00:00:00.000Z".to_string(),
        title: "Seeded inventory product".to_string(),
        handle: "seeded-inventory-product".to_string(),
        status: "ACTIVE".to_string(),
        description_html: String::new(),
        vendor: String::new(),
        product_type: String::new(),
        tags: Vec::new(),
        template_suffix: String::new(),
        seo_title: String::new(),
        seo_description: String::new(),
        ..ProductRecord::default()
    }
}

fn inventory_seed_proxy() -> DraftProxy {
    snapshot_proxy().with_base_products(vec![inventory_activation_base_product()])
}

fn create_inventory_test_item(proxy: &mut DraftProxy, sku: &str) -> (String, String) {
    let variant = create_legacy_variant(proxy, "gid://shopify/Product/1", sku, "10.00");
    (
        variant["id"].as_str().unwrap().to_string(),
        variant["inventoryItem"]["id"].as_str().unwrap().to_string(),
    )
}

fn add_inventory_test_location(proxy: &mut DraftProxy, name: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation AddInventoryLocation($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id name isActive }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "name": name, "address": { "countryCode": "US" } } }),
    ));
    assert_eq!(
        response.body["data"]["locationAdd"]["userErrors"],
        json!([])
    );
    response.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn inventory_level_id_for_test(inventory_item_id: &str, location_id: &str) -> String {
    let item_tail = inventory_item_id
        .rsplit('/')
        .next()
        .unwrap_or(inventory_item_id)
        .split('?')
        .next()
        .unwrap_or(inventory_item_id);
    let location_tail = location_id
        .rsplit('/')
        .next()
        .unwrap_or(location_id)
        .split('?')
        .next()
        .unwrap_or(location_id);
    format!(
        "gid://shopify/InventoryLevel/{item_tail}-{location_tail}?inventory_item_id={inventory_item_id}"
    )
}

fn assert_no_inventory_quantity_logs(proxy: &DraftProxy) {
    let blocked_roots = [
        "inventorySetQuantities",
        "inventoryAdjustQuantities",
        "inventoryMoveQuantities",
        "inventorySetOnHandQuantities",
    ];
    let entries = log_snapshot(proxy);
    let logged = entries["entries"].as_array().unwrap();
    assert!(
        logged.iter().all(|entry| {
            entry["interpreted"]["operationName"]
                .as_str()
                .map(|name| !blocked_roots.contains(&name))
                .unwrap_or(true)
        }),
        "rejected inventory quantity mutation should not be logged: {logged:?}"
    );
}

#[test]
fn inventory_activation_roots_stage_locally_and_read_inactive_levels() {
    use shopify_draft_proxy::proxy::UnsupportedMutationMode;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let calls = Arc::new(AtomicUsize::new(0));
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_base_products(vec![inventory_activation_base_product()])
    .with_upstream_transport({
        let calls = calls.clone();
        move |_request| {
            calls.fetch_add(1, Ordering::SeqCst);
            shopify_draft_proxy::proxy::Response {
                status: 599,
                headers: Default::default(),
                body: json!({"unexpectedUpstream": true}),
            }
        }
    });

    let variant = create_legacy_variant(
        &mut proxy,
        "gid://shopify/Product/1",
        "INV-ACTIVATE",
        "10.00",
    );
    let variant_id = variant["id"].as_str().unwrap().to_string();
    let inventory_item_id = variant["inventoryItem"]["id"].as_str().unwrap().to_string();
    let source_location_id = add_inventory_test_location(&mut proxy, "Source location");
    let second_location_id = add_inventory_test_location(&mut proxy, "Destination location");
    let source_level_id = inventory_level_id_for_test(&inventory_item_id, &source_location_id);
    calls.store(0, Ordering::SeqCst);

    let seed = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedInventory($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) { userErrors { field message code } }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "ignoreCompareQuantity": true, "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": source_location_id, "quantity": 5}
        ]}}),
    ));
    assert_eq!(
        seed.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let activate = proxy.process_request(json_graphql_request(
        r#"
        mutation ActivateInventoryLevel($inventoryItemId: ID!, $locationId: ID!) {
          inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId) {
            inventoryLevel {
              id
              isActive
              location { id name }
              quantities(names: ["available", "on_hand", "incoming"]) { name quantity updatedAt }
              item {
                id
                tracked
                variant { id inventoryQuantity product { id totalInventory tracksInventory } }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "locationId": second_location_id}),
    ));
    assert_eq!(
        activate.body["data"]["inventoryActivate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        activate.body["data"]["inventoryActivate"]["inventoryLevel"]["isActive"],
        json!(true)
    );
    assert_eq!(
        activate.body["data"]["inventoryActivate"]["inventoryLevel"]["quantities"],
        json!([
            {"name": "available", "quantity": 0, "updatedAt": "2024-01-01T00:00:01.000Z"},
            {"name": "on_hand", "quantity": 0, "updatedAt": null},
            {"name": "incoming", "quantity": 0, "updatedAt": null}
        ])
    );
    assert_eq!(
        activate.body["data"]["inventoryActivate"]["inventoryLevel"]["item"]["variant"]
            ["inventoryQuantity"],
        json!(5)
    );

    let deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation DeactivateInventoryLevel($inventoryLevelId: ID!) {
          inventoryDeactivate(inventoryLevelId: $inventoryLevelId) {
            userErrors { field message }
          }
        }
        "#,
        json!({"inventoryLevelId": source_level_id}),
    ));
    assert_eq!(
        deactivate.body["data"]["inventoryDeactivate"]["userErrors"],
        json!([])
    );

    let inactive_read = proxy.process_request(json_graphql_request(
        r#"
        query InactiveInventoryRead($inventoryItemId: ID!, $inventoryLevelId: ID!) {
          inventoryItem(id: $inventoryItemId) {
            inventoryLevels(first: 5) { nodes { location { id } isActive } }
            allLevels: inventoryLevels(first: 5, includeInactive: true) { nodes { location { id } isActive } }
          }
          inventoryLevel(id: $inventoryLevelId) {
            id
            isActive
            quantities(names: ["available", "incoming"]) { name quantity }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "inventoryLevelId": source_level_id}),
    ));
    assert_eq!(
        inactive_read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"],
        json!([{"location": {"id": second_location_id}, "isActive": true}])
    );
    assert_eq!(
        inactive_read.body["data"]["inventoryItem"]["allLevels"]["nodes"],
        json!([
            {"location": {"id": source_location_id}, "isActive": false},
            {"location": {"id": second_location_id}, "isActive": true}
        ])
    );
    assert_eq!(
        inactive_read.body["data"]["inventoryLevel"],
        json!({
            "id": source_level_id,
            "isActive": false,
            "quantities": [
                {"name": "available", "quantity": 5},
                {"name": "incoming", "quantity": 0}
            ]
        })
    );

    let bulk = proxy.process_request(json_graphql_request(
        r#"
        mutation BulkToggleActivation($inventoryItemId: ID!, $updates: [InventoryBulkToggleActivationInput!]!) {
          inventoryBulkToggleActivation(inventoryItemId: $inventoryItemId, inventoryItemUpdates: $updates) {
            inventoryItem {
              id
              inventoryLevels(first: 5) { nodes { location { id } isActive } }
            }
            inventoryLevels {
              location { id }
              quantities(names: ["available", "on_hand", "incoming"]) { name quantity }
              item { id tracked }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "updates": [
            {"locationId": source_location_id, "activate": true},
            {"locationId": second_location_id, "activate": false}
        ]}),
    ));
    assert_eq!(
        bulk.body["data"]["inventoryBulkToggleActivation"]["userErrors"],
        json!([])
    );
    assert_eq!(
        bulk.body["data"]["inventoryBulkToggleActivation"]["inventoryItem"]["inventoryLevels"]
            ["nodes"],
        json!([{"location": {"id": source_location_id}, "isActive": true}])
    );
    assert_eq!(
        bulk.body["data"]["inventoryBulkToggleActivation"]["inventoryLevels"][0]["quantities"],
        json!([
            {"name": "available", "quantity": 5},
            {"name": "on_hand", "quantity": 5},
            {"name": "incoming", "quantity": 0}
        ])
    );

    let item_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateInventoryItem($id: ID!, $input: InventoryItemInput!) {
          inventoryItemUpdate(id: $id, input: $input) {
            inventoryItem {
              id
              tracked
              requiresShipping
              countryCodeOfOrigin
              provinceCodeOfOrigin
              harmonizedSystemCode
              measurement { weight { value unit } }
              countryHarmonizedSystemCodes { countryCode harmonizedSystemCode }
              variant { id inventoryQuantity }
            }
            userErrors { field message  }
          }
        }
        "#,
        json!({"id": inventory_item_id, "input": {
            "tracked": false,
            "requiresShipping": false,
            "cost": "12.50",
            "countryCodeOfOrigin": "CA",
            "provinceCodeOfOrigin": "ON",
            "harmonizedSystemCode": "1234.56",
            "measurement": {"weight": {"value": 2.5, "unit": "KILOGRAMS"}},
            "countryHarmonizedSystemCodes": [{"countryCode": "US", "harmonizedSystemCode": "654321"}]
        }}),
    ));
    assert_eq!(
        item_update.body["data"]["inventoryItemUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        item_update.body["data"]["inventoryItemUpdate"]["inventoryItem"],
        json!({
            "id": inventory_item_id,
            "tracked": false,
            "requiresShipping": false,
            "countryCodeOfOrigin": "CA",
            "provinceCodeOfOrigin": "ON",
            "harmonizedSystemCode": "123456",
            "measurement": {"weight": {"value": 2.5, "unit": "KILOGRAMS"}},
            "countryHarmonizedSystemCodes": [{"countryCode": "US", "harmonizedSystemCode": "654321"}],
            "variant": {"id": variant_id, "inventoryQuantity": 5}
        })
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query InventoryItemUpdateDownstream($inventoryItemId: ID!) {
          inventoryItem(id: $inventoryItemId) { tracked requiresShipping harmonizedSystemCode }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id}),
    ));
    assert_eq!(
        downstream.body["data"]["inventoryItem"],
        json!({"tracked": false, "requiresShipping": false, "harmonizedSystemCode": "123456"})
    );
    let variant_downstream = proxy.process_request(json_graphql_request(
        r#"
        query InventoryItemUpdateVariantDownstream($variantId: ID!) {
          productVariant(id: $variantId) { inventoryItem { tracked requiresShipping harmonizedSystemCode } }
        }
        "#,
        json!({"variantId": variant_id}),
    ));
    assert_eq!(
        variant_downstream.body["data"]["productVariant"]["inventoryItem"],
        json!({"tracked": false, "requiresShipping": false, "harmonizedSystemCode": "123456"})
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn inventory_activate_on_hand_seeds_and_validates_locally() {
    use shopify_draft_proxy::proxy::UnsupportedMutationMode;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let calls = Arc::new(AtomicUsize::new(0));
    let mut proxy = configured_proxy(
        ReadMode::LiveHybrid,
        Some(UnsupportedMutationMode::Passthrough),
    )
    .with_base_products(vec![inventory_activation_base_product()])
    .with_upstream_transport({
        let calls = calls.clone();
        move |_request| {
            calls.fetch_add(1, Ordering::SeqCst);
            shopify_draft_proxy::proxy::Response {
                status: 599,
                headers: Default::default(),
                body: json!({"unexpectedUpstream": true}),
            }
        }
    });

    let variant = create_legacy_variant(
        &mut proxy,
        "gid://shopify/Product/1",
        "INV-ACTIVATE-ON-HAND",
        "10.00",
    );
    let inventory_item_id = variant["inventoryItem"]["id"].as_str().unwrap().to_string();
    let on_hand_location_id = add_inventory_test_location(&mut proxy, "On hand location");
    let conflict_location_id = add_inventory_test_location(&mut proxy, "Shop location");
    let out_of_range_location_id = add_inventory_test_location(&mut proxy, "Overflow location");
    let on_hand_level_id = inventory_level_id_for_test(&inventory_item_id, &on_hand_location_id);
    let conflict_level_id = inventory_level_id_for_test(&inventory_item_id, &conflict_location_id);
    calls.store(0, Ordering::SeqCst);

    let activate_on_hand = proxy.process_request(json_graphql_request(
        r#"
        mutation ActivateOnHand($inventoryItemId: ID!, $locationId: ID!, $onHand: Int) {
          inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, onHand: $onHand) {
            inventoryLevel {
              id
              isActive
              quantities(names: ["available", "on_hand"]) { name quantity updatedAt }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "locationId": on_hand_location_id, "onHand": 50}),
    ));
    assert_eq!(
        activate_on_hand.body["data"]["inventoryActivate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        activate_on_hand.body["data"]["inventoryActivate"]["inventoryLevel"]["quantities"],
        json!([
            {"name": "available", "quantity": 50, "updatedAt": "2024-01-01T00:00:01.000Z"},
            {"name": "on_hand", "quantity": 50, "updatedAt": null}
        ])
    );

    let downstream_on_hand = proxy.process_request(json_graphql_request(
        r#"
        query ActivatedOnHandRead($inventoryLevelId: ID!) {
          inventoryLevel(id: $inventoryLevelId) {
            isActive
            quantities(names: ["on_hand"]) { name quantity }
          }
        }
        "#,
        json!({"inventoryLevelId": on_hand_level_id}),
    ));
    assert_eq!(
        downstream_on_hand.body["data"]["inventoryLevel"],
        json!({
            "isActive": true,
            "quantities": [{"name": "on_hand", "quantity": 50}]
        })
    );

    let conflict = proxy.process_request(json_graphql_request(
        r#"
        mutation ActivateConflict($inventoryItemId: ID!, $locationId: ID!) {
          inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, available: 10, onHand: 20) {
            inventoryLevel { id }
            userErrors { field message }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "locationId": conflict_location_id}),
    ));
    assert_eq!(
        conflict.body["data"]["inventoryActivate"],
        json!({
            "inventoryLevel": null,
            "userErrors": [
                {
                    "field": ["available"],
                    "message": "The product couldn't be stocked at Shop location because not allowed to set available and on_hand quantities at the same time."
                },
                {
                    "field": ["onHand"],
                    "message": "The product couldn't be stocked at Shop location because not allowed to set available and on_hand quantities at the same time."
                }
            ]
        })
    );
    let conflict_downstream = proxy.process_request(json_graphql_request(
        r#"
        query ConflictLevelRead($inventoryLevelId: ID!) {
          inventoryLevel(id: $inventoryLevelId) { id }
        }
        "#,
        json!({"inventoryLevelId": conflict_level_id}),
    ));
    assert_eq!(
        conflict_downstream.body["data"]["inventoryLevel"],
        Value::Null
    );

    let already_active = proxy.process_request(json_graphql_request(
        r#"
        mutation ActivateOnHandAlreadyActive($inventoryItemId: ID!, $locationId: ID!) {
          inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, onHand: 5) {
            inventoryLevel { id }
            userErrors { field message }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "locationId": on_hand_location_id}),
    ));
    assert_eq!(
        already_active.body["data"]["inventoryActivate"]["userErrors"],
        json!([{
            "field": ["onHand"],
            "message": "Not allowed to set an on_hand quantity when the item is already active at the location."
        }])
    );
    let unchanged_on_hand = proxy.process_request(json_graphql_request(
        r#"
        query AlreadyActiveOnHandRead($inventoryLevelId: ID!) {
          inventoryLevel(id: $inventoryLevelId) {
            quantities(names: ["on_hand"]) { name quantity }
          }
        }
        "#,
        json!({"inventoryLevelId": on_hand_level_id}),
    ));
    assert_eq!(
        unchanged_on_hand.body["data"]["inventoryLevel"]["quantities"],
        json!([{"name": "on_hand", "quantity": 50}])
    );

    let out_of_range = proxy.process_request(json_graphql_request(
        r#"
        mutation ActivateOnHandOutOfRange($inventoryItemId: ID!, $locationId: ID!) {
          inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, onHand: 1000000001) {
            inventoryLevel { id }
            userErrors { field message }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "locationId": out_of_range_location_id}),
    ));
    assert_eq!(
        out_of_range.body["data"]["inventoryActivate"],
        json!({
            "inventoryLevel": null,
            "userErrors": [{
                "field": ["onHand"],
                "message": "The product couldn't be stocked at Overflow location because the quantity needs to be between -1 billion and 1 billion."
            }]
        })
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn inventory_activation_and_item_update_validation_errors_are_local() {
    let mut proxy = snapshot_proxy().with_base_products(vec![inventory_activation_base_product()]);
    let variant = create_legacy_variant(
        &mut proxy,
        "gid://shopify/Product/1",
        "INV-VALIDATION",
        "10.00",
    );
    let inventory_item_id = variant["inventoryItem"]["id"].as_str().unwrap().to_string();
    let location_id = add_inventory_test_location(&mut proxy, "Source location");
    let level_id = inventory_level_id_for_test(&inventory_item_id, &location_id);

    let seed = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedInventory($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) { userErrors { field message code } }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "ignoreCompareQuantity": true, "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": 1}
        ]}}),
    ));
    assert_eq!(
        seed.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let invalid_activate = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidActivate($inventoryItemId: ID!, $locationId: ID!, $available: Int) {
          inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, available: $available) {
            inventoryLevel { id }
            userErrors { field message }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "locationId": "gid://shopify/Location/848484848484", "available": -1}),
    ));
    let activate_errors = invalid_activate.body["data"]["inventoryActivate"]["userErrors"]
        .as_array()
        .unwrap();
    assert!(activate_errors.contains(&json!({
        "field": ["available"],
        "message": "Available must be greater than or equal to 0"
    })));
    assert!(activate_errors.contains(&json!({
        "field": ["locationId"],
        "message": "The product couldn't be stocked because the location wasn't found."
    })));
    let duplicate_activate = proxy.process_request(json_graphql_request(
        r#"
        mutation DuplicateActivate($inventoryItemId: ID!, $locationId: ID!) {
          inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, available: 1) {
            userErrors { field message }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "locationId": location_id}),
    ));
    assert_eq!(
        duplicate_activate.body["data"]["inventoryActivate"]["userErrors"],
        json!([{
            "field": ["available"],
            "message": "Not allowed to set available quantity when the item is already active at the location."
        }])
    );

    let last_location_deactivate = proxy.process_request(json_graphql_request(
        r#"
        mutation LastLocationDeactivate($inventoryLevelId: ID!) {
          inventoryDeactivate(inventoryLevelId: $inventoryLevelId) {
            userErrors { field message }
          }
        }
        "#,
        json!({"inventoryLevelId": level_id}),
    ));
    assert_eq!(
        last_location_deactivate.body["data"]["inventoryDeactivate"]["userErrors"],
        json!([{
            "field": null,
            "message": "The product couldn't be unstocked from Source location because products need to be stocked at a minimum of 1 location."
        }])
    );

    let mut activate_code_selection_request = json_graphql_request(
        r#"
        mutation InvalidActivateCodeSelection($inventoryItemId: ID!, $locationId: ID!, $available: Int) {
          inventoryActivate(inventoryItemId: $inventoryItemId, locationId: $locationId, available: $available) {
            userErrors { field message code }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "locationId": location_id, "available": -1}),
    );
    activate_code_selection_request.path = "/admin/api/2025-01/graphql.json".to_string();
    let activate_code_selection = proxy.process_request(activate_code_selection_request);
    assert_eq!(
        activate_code_selection.body["errors"][0]["extensions"],
        json!({
            "code": "undefinedField",
            "typeName": "UserError",
            "fieldName": "code"
        })
    );
    assert_eq!(
        activate_code_selection.body["errors"][0]["path"],
        json!([
            "mutation InvalidActivateCodeSelection",
            "inventoryActivate",
            "userErrors",
            "code"
        ])
    );

    let mut deactivate_code_selection_request = json_graphql_request(
        r#"
        mutation InvalidDeactivateCodeSelection($inventoryLevelId: ID!) {
          inventoryDeactivate(inventoryLevelId: $inventoryLevelId) {
            userErrors { field message code }
          }
        }
        "#,
        json!({"inventoryLevelId": level_id}),
    );
    deactivate_code_selection_request.path = "/admin/api/2025-01/graphql.json".to_string();
    let deactivate_code_selection = proxy.process_request(deactivate_code_selection_request);
    assert_eq!(
        deactivate_code_selection.body["errors"][0]["extensions"],
        json!({
            "code": "undefinedField",
            "typeName": "UserError",
            "fieldName": "code"
        })
    );
    assert_eq!(
        deactivate_code_selection.body["errors"][0]["path"],
        json!([
            "mutation InvalidDeactivateCodeSelection",
            "inventoryDeactivate",
            "userErrors",
            "code"
        ])
    );

    let invalid_bulk = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidBulk($inventoryItemId: ID!, $updates: [InventoryBulkToggleActivationInput!]!) {
          inventoryBulkToggleActivation(inventoryItemId: $inventoryItemId, inventoryItemUpdates: $updates) {
            userErrors { field message code }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "updates": [
            {"locationId": location_id, "activate": false}
        ]}),
    ));
    assert_eq!(
        invalid_bulk.body["data"]["inventoryBulkToggleActivation"]["userErrors"][0]["code"],
        json!("CANNOT_DEACTIVATE_FROM_ONLY_LOCATION")
    );

    let extra_quantity_field_bulk = proxy.process_request(json_graphql_request(
        r#"
        mutation ExtraQuantityFieldBulk($inventoryItemId: ID!, $updates: [InventoryBulkToggleActivationInput!]!) {
          inventoryBulkToggleActivation(inventoryItemId: $inventoryItemId, inventoryItemUpdates: $updates) {
            userErrors { field message code }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "updates": [
            {"locationId": location_id, "activate": true, "available": -1}
        ]}),
    ));
    assert_eq!(
        extra_quantity_field_bulk.body["data"]["inventoryBulkToggleActivation"]["userErrors"],
        json!([])
    );

    let inactive_location_id = add_active_transfer_location(&mut proxy, "Inactive bulk location");
    let make_inactive = proxy.process_request(json_graphql_request(
        r#"
        mutation MakeBulkLocationInactive($id: ID!, $input: LocationEditInput!) {
          locationEdit(id: $id, input: $input) {
            location { id isActive }
            userErrors { field message code }
          }
        }
        "#,
        json!({"id": inactive_location_id, "input": {"isActive": false}}),
    ));
    assert_eq!(
        make_inactive.body["data"]["locationEdit"]["userErrors"],
        json!([])
    );
    assert_eq!(
        make_inactive.body["data"]["locationEdit"]["location"]["isActive"],
        json!(false)
    );
    let inactive_bulk = proxy.process_request(json_graphql_request(
        r#"
        mutation InactiveLocationBulk($inventoryItemId: ID!, $updates: [InventoryBulkToggleActivationInput!]!) {
          inventoryBulkToggleActivation(inventoryItemId: $inventoryItemId, inventoryItemUpdates: $updates) {
            inventoryItem { id }
            inventoryLevels { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"inventoryItemId": inventory_item_id, "updates": [
            {"locationId": inactive_location_id, "activate": true}
        ]}),
    ));
    assert_eq!(
        inactive_bulk.body["data"]["inventoryBulkToggleActivation"],
        json!({
            "inventoryItem": null,
            "inventoryLevels": null,
            "userErrors": [{
                "field": ["inventoryItemUpdates", "0", "locationId"],
                "message": "The quantity couldn't be updated because the location was not found.",
                "code": "LOCATION_NOT_FOUND"
            }]
        })
    );

    let invalid_item_update = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidInventoryItemUpdate($id: ID!, $input: InventoryItemInput!) {
          inventoryItemUpdate(id: $id, input: $input) {
            inventoryItem { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({"id": inventory_item_id, "input": {
            "cost": "-5.00",
            "countryCodeOfOrigin": "US",
            "provinceCodeOfOrigin": "ONTARIO",
            "harmonizedSystemCode": "abc",
            "measurement": {"weight": {"value": -1, "unit": "KILOGRAMS"}},
            "countryHarmonizedSystemCodes": [
                {"countryCode": "US", "harmonizedSystemCode": "123456"},
                {"countryCode": "US", "harmonizedSystemCode": "654321"}
            ]
        }}),
    ));
    let item_errors = invalid_item_update.body["data"]["inventoryItemUpdate"]["userErrors"]
        .as_array()
        .unwrap();
    assert!(item_errors.contains(&json!({
        "field": ["input", "cost"],
        "message": "Cost must be greater than or equal to 0"
    })));
    assert!(item_errors.contains(&json!({
        "field": ["input", "measurement", "weight"],
        "message": "Measurement weight value -1 kg must be >= 0 kg"
    })));
    assert!(item_errors.contains(&json!({
        "field": ["input", "provinceCodeOfOrigin"],
        "message": "Province code of origin is invalid"
    })));
    assert!(item_errors.contains(&json!({
        "field": ["input", "harmonizedSystemCode"],
        "message": "Harmonized system code must be a number between six and thirteen digits"
    })));
    assert!(item_errors.contains(&json!({
        "field": ["input", "countryHarmonizedSystemCodes", "1", "countryCode"],
        "message": "Country code has already been taken"
    })));

    let invalid_unit = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidWeightUnit($id: ID!, $input: InventoryItemInput!) {
          inventoryItemUpdate(id: $id, input: $input) {
            inventoryItem { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({"id": inventory_item_id, "input": {
            "measurement": {"weight": {"value": 1, "unit": "STONE"}}
        }}),
    ));
    assert_eq!(
        invalid_unit.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );

    let missing_item = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingInventoryItem($id: ID!, $input: InventoryItemInput!) {
          inventoryItemUpdate(id: $id, input: $input) {
            inventoryItem { id }
            userErrors { field message  }
          }
        }
        "#,
        json!({"id": "gid://shopify/InventoryItem/959595959595", "input": {"tracked": true}}),
    ));
    assert_eq!(
        missing_item.body["data"]["inventoryItemUpdate"]["inventoryItem"],
        Value::Null
    );
    assert_eq!(
        missing_item.body["data"]["inventoryItemUpdate"]["userErrors"],
        json!([{
            "field": ["id"],
            "message": "The product couldn't be updated because it does not exist."
        }])
    );
}

#[test]
fn inventory_quantity_name_validation_rejects_invalid_names_without_staging() {
    let mut proxy = snapshot_proxy();
    let public_name_message = "The specified quantity name is invalid. Valid values are: available, damaged, incoming, quality_control, reserved, safety_stock.";

    let adjust = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidAdjustName($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
          inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { reason }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "invalid-adjust-name", "input": {"name": "on_hand", "reason": "correction", "referenceDocumentUri": "logistics://inventory/name/adjust", "changes": [
            {"inventoryItemId": "gid://shopify/InventoryItem/name-validation", "locationId": "gid://shopify/Location/1", "delta": 1, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        adjust.body["data"]["inventoryAdjustQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "name"],
                "message": public_name_message,
                "code": "INVALID_QUANTITY_NAME"
            }]
        })
    );

    let set = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidSetName($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { reason }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "invalid-set-name", "input": {"name": "committed", "reason": "correction", "referenceDocumentUri": "logistics://inventory/name/set", "quantities": [
            {"inventoryItemId": "gid://shopify/InventoryItem/name-validation", "locationId": "gid://shopify/Location/1", "quantity": 5, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        set.body["data"]["inventorySetQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "name"],
                "message": "The quantity name must be either 'available' or 'on_hand'.",
                "code": "INVALID_NAME"
            }]
        })
    );

    let set_too_high = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidSetQuantity($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { reason }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "invalid-set-quantity", "input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://inventory/name/set-too-high", "quantities": [
            {"inventoryItemId": "gid://shopify/InventoryItem/name-validation", "locationId": "gid://shopify/Location/1", "quantity": 1000000001, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        set_too_high.body["data"]["inventorySetQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "quantities", "0", "quantity"],
                "message": "The quantity can't be higher than 1,000,000,000.",
                "code": "INVALID_QUANTITY_TOO_HIGH"
            }]
        })
    );

    let set_duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidSetDuplicate($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { reason }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "invalid-set-duplicate", "input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://inventory/name/set-duplicate", "quantities": [
            {"inventoryItemId": "gid://shopify/InventoryItem/name-validation", "locationId": "gid://shopify/Location/1", "quantity": 2, "changeFromQuantity": 0},
            {"inventoryItemId": "gid://shopify/InventoryItem/name-validation", "locationId": "gid://shopify/Location/1", "quantity": 3, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        set_duplicate.body["data"]["inventorySetQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [
                {
                    "field": ["input", "quantities", "0", "locationId"],
                    "message": "The combination of inventoryItemId and locationId must be unique.",
                    "code": "NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR"
                },
                {
                    "field": ["input", "quantities", "1", "locationId"],
                    "message": "The combination of inventoryItemId and locationId must be unique.",
                    "code": "NO_DUPLICATE_INVENTORY_ITEM_ID_GROUP_ID_PAIR"
                }
            ]
        })
    );

    let move_from = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidMoveFromName($input: InventoryMoveQuantitiesInput!) {
          inventoryMoveQuantities(input: $input) {
            inventoryAdjustmentGroup { reason }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"reason": "correction", "referenceDocumentUri": "logistics://inventory/name/move-from", "changes": [{
            "inventoryItemId": "gid://shopify/InventoryItem/name-validation",
            "quantity": 1,
            "from": {"locationId": "gid://shopify/Location/1", "name": "on_hand"},
            "to": {"locationId": "gid://shopify/Location/1", "name": "damaged", "ledgerDocumentUri": "ledger://inventory/name/move-from"}
        }]}}),
    ));
    assert_eq!(
        move_from.body["data"]["inventoryMoveQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "changes", "0", "from", "name"],
                "message": public_name_message,
                "code": "INVALID_QUANTITY_NAME"
            }]
        })
    );

    let move_to = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidMoveToName($input: InventoryMoveQuantitiesInput!) {
          inventoryMoveQuantities(input: $input) {
            inventoryAdjustmentGroup { reason }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {"reason": "correction", "referenceDocumentUri": "logistics://inventory/name/move-to", "changes": [{
            "inventoryItemId": "gid://shopify/InventoryItem/name-validation",
            "quantity": 1,
            "from": {"locationId": "gid://shopify/Location/1", "name": "available"},
            "to": {"locationId": "gid://shopify/Location/1", "name": "committed", "ledgerDocumentUri": "ledger://inventory/name/move-to"}
        }]}}),
    ));
    assert_eq!(
        move_to.body["data"]["inventoryMoveQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "changes", "0", "to", "name"],
                "message": public_name_message,
                "code": "INVALID_QUANTITY_NAME"
            }]
        })
    );

    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn inventory_adjust_quantities_ledger_document_validation_rejects_without_staging() {
    let mut proxy = inventory_seed_proxy();
    let mutation = r#"
        mutation LedgerDocumentAdjust($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
          inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup {
              id
              changes { name delta ledgerDocumentUri }
            }
            userErrors { field message code }
          }
        }
        "#;

    let missing_non_available = proxy.process_request(json_graphql_request(
        mutation,
        json!({"idempotencyKey": "ledger-required-non-available", "input": {"name": "damaged", "reason": "damaged", "changes": [
            {"inventoryItemId": "gid://shopify/InventoryItem/ledger-required", "locationId": "gid://shopify/Location/1", "delta": 5, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        missing_non_available.body["data"]["inventoryAdjustQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "changes", "0", "ledgerDocumentUri"],
                "message": "A ledger document URI is required except when adjusting available.",
                "code": "INVALID_QUANTITY_DOCUMENT"
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    assert!(state_snapshot(&proxy)["stagedState"]["inventoryLevels"].is_null());

    let available_with_ledger = proxy.process_request(json_graphql_request(
        mutation,
        json!({"idempotencyKey": "ledger-forbidden-available", "input": {"name": "available", "reason": "correction", "changes": [
            {"inventoryItemId": "gid://shopify/InventoryItem/ledger-forbidden", "locationId": "gid://shopify/Location/1", "delta": 5, "changeFromQuantity": 0, "ledgerDocumentUri": "https://example.com/doc/1"}
        ]}}),
    ));
    assert_eq!(
        available_with_ledger.body["data"]["inventoryAdjustQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "changes", "0", "ledgerDocumentUri"],
                "message": "A ledger document URI is not allowed when adjusting available.",
                "code": "INVALID_AVAILABLE_DOCUMENT"
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    assert!(state_snapshot(&proxy)["stagedState"]["inventoryLevels"].is_null());

    let internal_gid_ledger = proxy.process_request(json_graphql_request(
        mutation,
        json!({"idempotencyKey": "ledger-internal-gid", "input": {"name": "reserved", "reason": "correction", "changes": [
            {"inventoryItemId": "gid://shopify/InventoryItem/ledger-internal", "locationId": "gid://shopify/Location/1", "delta": 5, "changeFromQuantity": 0, "ledgerDocumentUri": "gid://shopify/Order/123"}
        ]}}),
    ));
    assert_eq!(
        internal_gid_ledger.body["data"]["inventoryAdjustQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "changes", "0", "ledgerDocumentUri"],
                "message": "Internal (gid://shopify/) ledger documents are not allowed to be adjusted via API.",
                "code": "INTERNAL_LEDGER_DOCUMENT"
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    assert!(state_snapshot(&proxy)["stagedState"]["inventoryLevels"].is_null());

    let multiple_distinct_ledgers = proxy.process_request(json_graphql_request(
        mutation,
        json!({"idempotencyKey": "ledger-max-one-document", "input": {"name": "damaged", "reason": "damaged", "changes": [
            {"inventoryItemId": "gid://shopify/InventoryItem/ledger-first", "locationId": "gid://shopify/Location/1", "delta": 5, "changeFromQuantity": 0, "ledgerDocumentUri": "https://example.com/doc/1"},
            {"inventoryItemId": "gid://shopify/InventoryItem/ledger-second", "locationId": "gid://shopify/Location/1", "delta": 6, "changeFromQuantity": 0, "ledgerDocumentUri": "https://example.com/doc/2"}
        ]}}),
    ));
    assert_eq!(
        multiple_distinct_ledgers.body["data"]["inventoryAdjustQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "changes"],
                "message": "All changes must have the same ledger document URI or, in the case of adjusting available, no ledger document URI.",
                "code": "MAX_ONE_LEDGER_DOCUMENT"
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    assert!(state_snapshot(&proxy)["stagedState"]["inventoryLevels"].is_null());

    let (_variant_id, valid_inventory_item_id) =
        create_inventory_test_item(&mut proxy, "LEDGER-VALID");
    let valid_location_id = add_inventory_test_location(&mut proxy, "Ledger valid location");
    let valid_non_available = proxy.process_request(json_graphql_request(
        mutation,
        json!({"idempotencyKey": "ledger-valid-non-available", "input": {"name": "incoming", "reason": "received", "changes": [
            {"inventoryItemId": valid_inventory_item_id, "locationId": valid_location_id, "delta": 5, "changeFromQuantity": 0, "ledgerDocumentUri": "https://example.com/doc/valid"}
        ]}}),
    ));
    let valid_payload = &valid_non_available.body["data"]["inventoryAdjustQuantities"];
    assert_eq!(valid_payload["userErrors"], json!([]));
    assert_ne!(valid_payload["inventoryAdjustmentGroup"], Value::Null);
    assert_eq!(
        valid_payload["inventoryAdjustmentGroup"]["changes"][0]["ledgerDocumentUri"],
        json!("https://example.com/doc/valid")
    );
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["inventoryLevels"][0]["quantities"]["incoming"],
        json!(5)
    );
    let log = log_snapshot(&proxy);
    let adjust_log = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["interpreted"]["operationName"] == json!("inventoryAdjustQuantities"))
        .expect("inventoryAdjustQuantities should be logged");
    assert_eq!(adjust_log["status"], json!("staged"));
    assert!(adjust_log["rawBody"]
        .as_str()
        .unwrap()
        .contains("https://example.com/doc/valid"));
}

#[test]
fn inventory_set_quantities_rejects_bounds_before_staging_and_allows_available_negative() {
    let mut proxy = inventory_seed_proxy();
    let (_variant_id, inventory_item_id) = create_inventory_test_item(&mut proxy, "BOUNDS");
    let location_id = add_inventory_test_location(&mut proxy, "Bounds location");
    let mutation = r#"
        mutation InventorySetQuantitiesBounds($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { reason changes { name delta } }
            userErrors { field message code }
          }
        }
        "#;

    let on_hand_negative = proxy.process_request(json_graphql_request(
        mutation,
        json!({"idempotencyKey": "set-on-hand-negative-bound", "input": {"name": "on_hand", "reason": "correction", "referenceDocumentUri": "logistics://inventory/bounds/on-hand-negative", "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": -5, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        on_hand_negative.body["data"]["inventorySetQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "quantities", "0", "quantity"],
                "message": "The quantity can't be negative.",
                "code": "INVALID_QUANTITY_NEGATIVE"
            }]
        })
    );

    let on_hand_too_low = proxy.process_request(json_graphql_request(
        mutation,
        json!({"idempotencyKey": "set-on-hand-too-low-bound", "input": {"name": "on_hand", "reason": "correction", "referenceDocumentUri": "logistics://inventory/bounds/on-hand-too-low", "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": -2000000000, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        on_hand_too_low.body["data"]["inventorySetQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "quantities", "0", "quantity"],
                "message": "The quantity can't be lower than -1,000,000,000.",
                "code": "INVALID_QUANTITY_TOO_LOW"
            }]
        })
    );

    let available_too_low = proxy.process_request(json_graphql_request(
        mutation,
        json!({"idempotencyKey": "set-available-too-low-bound", "input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://inventory/bounds/available-too-low", "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": -2000000000, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        available_too_low.body["data"]["inventorySetQuantities"],
        json!({
            "inventoryAdjustmentGroup": null,
            "userErrors": [{
                "field": ["input", "quantities", "0", "quantity"],
                "message": "The quantity can't be lower than -1,000,000,000.",
                "code": "INVALID_QUANTITY_TOO_LOW"
            }]
        })
    );
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["inventoryLevels"],
        Value::Null
    );
    assert_no_inventory_quantity_logs(&proxy);

    let available_negative = proxy.process_request(json_graphql_request(
        mutation,
        json!({"idempotencyKey": "set-available-negative-bound", "input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://inventory/bounds/available-negative", "quantities": [
            {"inventoryItemId": inventory_item_id, "locationId": location_id, "quantity": -5, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        available_negative.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );
    assert_ne!(
        available_negative.body["data"]["inventorySetQuantities"]["inventoryAdjustmentGroup"],
        Value::Null
    );
    let state = state_snapshot(&proxy);
    assert_eq!(
        state["stagedState"]["inventoryLevels"][0]["quantities"]["available"],
        json!(-5)
    );
    assert_eq!(
        state["stagedState"]["inventoryLevels"][0]["quantities"]["on_hand"],
        json!(-5)
    );
    let log = log_snapshot(&proxy);
    let quantity_logs: Vec<_> = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["interpreted"]["operationName"] == json!("inventorySetQuantities"))
        .collect();
    assert_eq!(quantity_logs.len(), 1);
    assert_eq!(quantity_logs[0]["status"], json!("staged"));
    assert!(quantity_logs[0]["rawBody"]
        .as_str()
        .unwrap()
        .contains("\"quantity\":-5"));
}

#[test]
fn inventory_quantity_2026_missing_change_from_returns_graphql_error_without_staging() {
    let mut proxy = snapshot_proxy();

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingChangeFrom($input: InventoryAdjustQuantitiesInput!, $idempotencyKey: String!) {
          inventoryAdjustQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { reason }
            userErrors { field message }
          }
        }
        "#,
        json!({"idempotencyKey": "inventory-adjust-missing-change-from", "input": {"name": "available", "reason": "correction", "changes": [
            {"inventoryItemId": "gid://shopify/InventoryItem/474747474747", "locationId": "gid://shopify/Location/1", "delta": 1}
        ]}}),
    ));

    assert_eq!(
        response.body["data"]["inventoryAdjustQuantities"],
        Value::Null
    );
    assert_eq!(
        response.body["errors"][0]["message"],
        json!("InventoryChangeInput must include the following argument: changeFromQuantity.")
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
}

#[test]
fn order_create_decrements_inventory_when_inventory_behaviour_is_not_bypass() {
    let mut proxy = inventory_seed_proxy();
    let (decrement_variant_id, decrement_inventory_item_id) =
        create_inventory_test_item(&mut proxy, "ORDER-DECREMENT");
    let (bypass_variant_id, bypass_inventory_item_id) =
        create_inventory_test_item(&mut proxy, "ORDER-BYPASS");
    let location_id = add_inventory_test_location(&mut proxy, "Order location");

    let seed = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedInventory($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            userErrors { field message }
          }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://inventory/order-create-seed", "ignoreCompareQuantity": true, "quantities": [
            {"inventoryItemId": decrement_inventory_item_id, "locationId": location_id, "quantity": 5}
        ]}}),
    ));
    assert_eq!(
        seed.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let order = proxy.process_request(json_graphql_request(
        r#"
        mutation OrderCreateInventoryDecrement($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
          orderCreate(order: $order, options: $options) {
            order {
              id
              lineItems(first: 5) {
                nodes {
                  id
                  quantity
                  variant { id }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "inventory-decrement@example.com",
                "currency": "USD",
                "lineItems": [{
                    "variantId": decrement_variant_id,
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            },
            "options": {
                "sendReceipt": false,
                "sendFulfillmentReceipt": false
            }
        }),
    ));
    assert_eq!(order.body["data"]["orderCreate"]["userErrors"], json!([]));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query InventoryAfterOrderCreate($id: ID!) {
          inventoryItem(id: $id) {
            variant { inventoryQuantity }
            inventoryLevels(first: 5) {
              nodes {
                quantities(names: ["available", "on_hand"]) { name quantity }
              }
            }
          }
        }
        "#,
        json!({"id": decrement_inventory_item_id}),
    ));
    assert_eq!(
        read.body["data"]["inventoryItem"]["variant"]["inventoryQuantity"],
        json!(3)
    );
    assert_eq!(
        read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"],
        json!([
            {"name": "available", "quantity": 3},
            {"name": "on_hand", "quantity": 5}
        ])
    );
    let log = log_snapshot(&proxy);
    let order_log = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["interpreted"]["operationName"] == json!("orderCreate"))
        .expect("orderCreate should be logged");
    assert_eq!(order_log["status"], json!("staged"));
    assert_eq!(
        order_log["interpreted"]["capability"],
        json!({
            "operationName": "orderCreate",
            "domain": "orders",
            "execution": "stage-locally"
        })
    );
    assert_eq!(
        order_log["notes"],
        json!("Locally staged orderCreate in shopify-draft-proxy.")
    );
    assert_eq!(
        order_log["stagedResourceIds"],
        json!(["gid://shopify/Order/1"])
    );

    let bypass_seed = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedInventory($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            userErrors { field message }
          }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://inventory/order-create-bypass-seed", "ignoreCompareQuantity": true, "quantities": [
            {"inventoryItemId": bypass_inventory_item_id, "locationId": location_id, "quantity": 8}
        ]}}),
    ));
    assert_eq!(
        bypass_seed.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let bypass_order = proxy.process_request(json_graphql_request(
        r#"
        mutation OrderCreateInventoryBypass($order: OrderCreateOrderInput!, $options: OrderCreateOptionsInput) {
          orderCreate(order: $order, options: $options) {
            order { id lineItems(first: 5) { nodes { quantity variant { id } } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "order": {
                "email": "inventory-bypass@example.com",
                "currency": "USD",
                "lineItems": [{
                    "variantId": bypass_variant_id,
                    "quantity": 4,
                    "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                }]
            },
            "options": {
                "inventoryBehaviour": "BYPASS",
                "sendReceipt": false,
                "sendFulfillmentReceipt": false
            }
        }),
    ));
    assert_eq!(
        bypass_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );

    let bypass_read = proxy.process_request(json_graphql_request(
        r#"
        query InventoryAfterOrderCreate($id: ID!) {
          inventoryItem(id: $id) {
            variant { inventoryQuantity }
            inventoryLevels(first: 5) {
              nodes {
                quantities(names: ["available", "on_hand"]) { name quantity }
              }
            }
          }
        }
        "#,
        json!({"id": bypass_inventory_item_id}),
    ));
    assert_eq!(
        bypass_read.body["data"]["inventoryItem"]["variant"]["inventoryQuantity"],
        json!(8)
    );
    assert_eq!(
        bypass_read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"],
        json!([
            {"name": "available", "quantity": 8},
            {"name": "on_hand", "quantity": 8}
        ])
    );
}

/// Adds a real, active location via the public `locationAdd` mutation and returns
/// its freshly-minted gid. Transfer endpoints must resolve to active locations in
/// staged store state, so each transfer scenario seeds its origin/destination this
/// way rather than leaning on capture-specific location ids being implicitly valid.
fn add_active_transfer_location(proxy: &mut DraftProxy, name: &str) -> String {
    let add = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedTransferLocation($input: LocationAddInput!) {
          locationAdd(input: $input) {
            location { id isActive }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {
            "name": name,
            "address": {
                "countryCode": "US",
                "address1": "1 Transfer Way",
                "city": "New York",
                "zip": "10001"
            }
        }}),
    ));
    assert_eq!(
        add.body["data"]["locationAdd"]["userErrors"],
        json!([]),
        "locationAdd should succeed while seeding a transfer endpoint"
    );
    assert_eq!(
        add.body["data"]["locationAdd"]["location"]["isActive"],
        json!(true)
    );
    add.body["data"]["locationAdd"]["location"]["id"]
        .as_str()
        .expect("seeded location must have an id")
        .to_string()
}

/// Stocks `inventory_item_id` at `location_id` with the given available quantity via
/// `inventoryActivate`, establishing the origin inventory level a transfer needs in
/// order to pass its "item is stocked at origin" validation.
fn stock_transfer_item_at_origin(
    proxy: &mut DraftProxy,
    inventory_item_id: &str,
    location_id: &str,
    available: i64,
) {
    let activate = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedTransferStock($inventoryItemId: ID!, $locationId: ID!, $available: Int!) {
          inventoryActivate(
            inventoryItemId: $inventoryItemId
            locationId: $locationId
            available: $available
          ) {
            inventoryLevel { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "inventoryItemId": inventory_item_id,
            "locationId": location_id,
            "available": available
        }),
    ));
    assert_eq!(
        activate.body["data"]["inventoryActivate"]["userErrors"],
        json!([]),
        "inventoryActivate should succeed while stocking a transfer item"
    );
}

/// Reads the staged inventory level quantities for `inventory_item_id` at
/// `location_id`. The transfer flow creates several levels (origin, destination, and
/// the default location) whose relative ordering depends on their minted location
/// ids, so callers locate the level they care about by location id rather than
/// assuming a particular position in the connection.
fn transfer_level_quantities(
    proxy: &mut DraftProxy,
    inventory_item_id: &str,
    location_id: &str,
) -> Value {
    let read = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/inventory-transfer-inventory-read-all-levels.graphql"
        ),
        json!({"id": inventory_item_id}),
    ));
    read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|node| node["location"]["id"] == json!(location_id))
        .map(|node| node["quantities"].clone())
        .unwrap_or(Value::Null)
}

/// Collects the logged root operation names that belong to the inventory-transfer
/// family, filtering out the location/inventory setup mutations a scenario stages
/// before exercising the transfer itself. A wrongly-logged `inventoryTransfer*`
/// operation still surfaces (the prefix match keeps the regression coverage).
fn transfer_log_roots(proxy: &DraftProxy) -> Vec<Value> {
    log_snapshot(proxy)["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["interpreted"]["operationName"].clone())
        .filter(|name| {
            name.as_str()
                .map(|n| n.starts_with("inventoryTransfer"))
                .unwrap_or(false)
        })
        .collect()
}

#[test]
fn inventory_transfer_create_keeps_empty_hydrated_origin_quantities_zero() {
    let origin_id = "gid://shopify/Location/111111111111";
    let destination_id = "gid://shopify/Location/222222222222";
    let inventory_item_id = "gid://shopify/InventoryItem/333333333333";
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(|_| {
        Response {
            status: 200,
            headers: Default::default(),
            body: json!({"data": {"nodes": [
                {
                    "__typename": "Location",
                    "id": "gid://shopify/Location/111111111111",
                    "name": "Zero Origin",
                    "isActive": true
                },
                {
                    "__typename": "Location",
                    "id": "gid://shopify/Location/222222222222",
                    "name": "Zero Destination",
                    "isActive": true
                },
                {
                    "__typename": "InventoryItem",
                    "id": "gid://shopify/InventoryItem/333333333333",
                    "tracked": true,
                    "requiresShipping": true,
                    "variant": {
                        "id": "gid://shopify/ProductVariant/333333333333",
                        "title": "Zero Transfer Variant",
                        "inventoryQuantity": 0,
                        "product": {
                            "id": "gid://shopify/Product/333333333333",
                            "title": "Zero Transfer Product",
                            "handle": "zero-transfer-product",
                            "status": "ACTIVE",
                            "totalInventory": 0,
                            "tracksInventory": true
                        }
                    },
                    "inventoryLevels": {"nodes": [{
                        "id": "gid://shopify/InventoryLevel/333333333333-111111111111?inventory_item_id=gid://shopify/InventoryItem/333333333333",
                        "location": {"id": "gid://shopify/Location/111111111111", "name": "Zero Origin"},
                        "quantities": []
                    }]}
                }
            ]}}),
        }
    });

    let create_response = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/inventory-transfer-create.graphql"),
        json!({"input": {
            "originLocationId": origin_id,
            "destinationLocationId": destination_id,
            "lineItems": [{"inventoryItemId": inventory_item_id, "quantity": 2}]
        }}),
    ));
    assert_eq!(
        create_response.body["data"]["inventoryTransferCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create_response.body["data"]["inventoryTransferCreate"]["inventoryTransfer"]["status"],
        json!("DRAFT")
    );

    assert_eq!(
        transfer_level_quantities(&mut proxy, inventory_item_id, origin_id),
        json!([
            {"name": "available", "quantity": 0},
            {"name": "reserved", "quantity": 0},
            {"name": "on_hand", "quantity": 0}
        ])
    );
}

#[test]
fn inventory_transfer_lifecycle_stages_and_updates_inventory_levels_from_store() {
    let mut proxy = inventory_seed_proxy();

    // The transfer engine validates that both endpoints are real, active locations
    // and that the moved item is stocked at the origin, then computes the reservation
    // math against those staged inventory levels. Seed that world state up front via
    // the same public mutations a merchant would use instead of relying on
    // capture-specific ids being treated as implicitly valid/stocked.
    let origin_id = add_active_transfer_location(&mut proxy, "Transfer Origin");
    let destination_id = add_active_transfer_location(&mut proxy, "Transfer Destination");
    let (_variant_id, inventory_item_id) =
        create_inventory_test_item(&mut proxy, "TRANSFER-LIFECYCLE");
    stock_transfer_item_at_origin(&mut proxy, &inventory_item_id, &origin_id, 5);

    let create_response = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/inventory-transfer-create.graphql"),
        json!({"input": {
            "originLocationId": origin_id,
            "destinationLocationId": destination_id,
            "lineItems": [{"inventoryItemId": inventory_item_id, "quantity": 2}]
        }}),
    ));
    assert_eq!(
        create_response.body["data"]["inventoryTransferCreate"]["inventoryTransfer"]["status"],
        json!("DRAFT")
    );
    let transfer_id = create_response.body["data"]["inventoryTransferCreate"]["inventoryTransfer"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    let ready_response = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/inventory-transfer-mark-ready.graphql"),
        json!({"id": transfer_id}),
    ));
    assert_eq!(
        ready_response.body["data"]["inventoryTransferMarkAsReadyToShip"]["inventoryTransfer"]
            ["status"],
        json!("READY_TO_SHIP")
    );
    assert_eq!(
        ready_response.body["data"]["inventoryTransferMarkAsReadyToShip"]["inventoryTransfer"]
            ["lineItems"]["nodes"][0]["shippableQuantity"],
        json!(2)
    );

    // The reservation moves 2 units out of available into reserved at the origin,
    // leaving on_hand untouched (available 5 -> 3, reserved 0 -> 2, on_hand 5).
    assert_eq!(
        transfer_level_quantities(&mut proxy, &inventory_item_id, &origin_id),
        json!([
            {"name": "available", "quantity": 3},
            {"name": "reserved", "quantity": 2},
            {"name": "on_hand", "quantity": 5}
        ])
    );

    let cancel_response = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/inventory-transfer-cancel.graphql"),
        json!({"id": transfer_id}),
    ));
    assert_eq!(
        cancel_response.body["data"]["inventoryTransferCancel"]["inventoryTransfer"]["status"],
        json!("CANCELED")
    );
    // Canceling releases the reservation back to available (3 -> 5, reserved 2 -> 0).
    assert_eq!(
        transfer_level_quantities(&mut proxy, &inventory_item_id, &origin_id),
        json!([
            {"name": "available", "quantity": 5},
            {"name": "reserved", "quantity": 0},
            {"name": "on_hand", "quantity": 5}
        ])
    );

    let delete_response = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/inventory-transfer-delete.graphql"),
        json!({"id": transfer_id}),
    ));
    assert_eq!(
        delete_response.body["data"]["inventoryTransferDelete"]["deletedId"],
        Value::Null
    );
    assert_eq!(
        delete_response.body["data"]["inventoryTransferDelete"]["userErrors"][0]["message"],
        json!("Can't delete the transfer if it's not in the draft status.")
    );

    assert_eq!(
        transfer_log_roots(&proxy),
        vec![
            json!("inventoryTransferCreate"),
            json!("inventoryTransferMarkAsReadyToShip"),
            json!("inventoryTransferCancel")
        ]
    );
}

#[test]
fn inventory_transfer_create_and_set_items_validate_before_staging() {
    let mut proxy = inventory_seed_proxy();

    // Seed two active locations and stock the moved item at the origin so the only
    // validation error in the same-location case below is the origin/destination
    // clash itself (not a "location not found" or "item not stocked" rejection).
    let origin_id = add_active_transfer_location(&mut proxy, "Validation Origin");
    let destination_id = add_active_transfer_location(&mut proxy, "Validation Destination");
    let (_variant_id, inventory_item_id) =
        create_inventory_test_item(&mut proxy, "TRANSFER-VALIDATION");
    stock_transfer_item_at_origin(&mut proxy, &inventory_item_id, &origin_id, 5);

    let create_validation = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/inventory-transfer-create-validation.graphql"
        ),
        json!({"input": {
            "originLocationId": origin_id,
            "destinationLocationId": origin_id,
            "lineItems": [{"inventoryItemId": inventory_item_id, "quantity": 1}]
        }}),
    ));
    assert_eq!(
        create_validation.body["data"]["inventoryTransferCreate"],
        json!({
            "inventoryTransfer": null,
            "userErrors": [{
                "field": ["input", "destinationLocationId"],
                "message": "The origin location cannot be the same as the destination location.",
                "code": "TRANSFER_ORIGIN_CANNOT_BE_THE_SAME_AS_DESTINATION"
            }]
        })
    );
    // The rejected create stages nothing — only the setup mutations are logged, no
    // transfer operation.
    assert_eq!(transfer_log_roots(&proxy), Vec::<Value>::new());

    let create_response = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/inventory-transfer-create.graphql"),
        json!({"input": {
            "originLocationId": origin_id,
            "destinationLocationId": destination_id,
            "lineItems": [{"inventoryItemId": inventory_item_id, "quantity": 2}]
        }}),
    ));
    assert_eq!(
        create_response.body["data"]["inventoryTransferCreate"]["userErrors"],
        json!([])
    );
    let transfer_id = create_response.body["data"]["inventoryTransferCreate"]["inventoryTransfer"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    let set_validation = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/inventory-transfer-set-items.graphql"),
        json!({"input": {
            "id": transfer_id,
            "lineItems": [
                {"inventoryItemId": inventory_item_id, "quantity": 1},
                {"inventoryItemId": inventory_item_id, "quantity": -1},
                {"inventoryItemId": "gid://shopify/InventoryItem/444444444444", "quantity": 1}
            ]
        }}),
    ));
    assert_eq!(
        set_validation.body["data"]["inventoryTransferSetItems"]["inventoryTransfer"],
        Value::Null
    );
    assert_eq!(
        set_validation.body["data"]["inventoryTransferSetItems"]["updatedLineItems"],
        Value::Null
    );
    assert_eq!(
        set_validation.body["data"]["inventoryTransferSetItems"]["userErrors"],
        json!([
            {
                "field": ["input", "lineItems", "0", "inventoryItemId"],
                "message": "The inventory item is already present in the list. Each item must be unique.",
                "code": "DUPLICATE_ITEM"
            },
            {
                "field": ["input", "lineItems", "1", "inventoryItemId"],
                "message": "The inventory item is already present in the list. Each item must be unique.",
                "code": "DUPLICATE_ITEM"
            },
            {
                "field": ["input", "lineItems", "1", "quantity"],
                "message": "The quantity can't be negative.",
                "code": "INVALID_QUANTITY"
            },
            {
                "field": ["input", "lineItems", "2", "inventoryItemId"],
                "message": "The inventory item could not be found.",
                "code": "ITEM_NOT_FOUND"
            }
        ])
    );

    let read_after_rejected_set = proxy.process_request(json_graphql_request(
        r#"
        query InventoryTransferAfterRejectedSet($id: ID!) {
          inventoryTransfer(id: $id) {
            totalQuantity
            lineItems(first: 10) { nodes { totalQuantity } }
          }
        }
        "#,
        json!({"id": transfer_id}),
    ));
    assert_eq!(
        read_after_rejected_set.body["data"]["inventoryTransfer"]["totalQuantity"],
        json!(2)
    );
    // Only the successful create is logged; the rejected set-items call stages
    // nothing.
    assert_eq!(
        transfer_log_roots(&proxy),
        vec![json!("inventoryTransferCreate")]
    );
}

#[test]
fn inventory_transfer_edit_and_duplicate_stage_locally_without_upstream_passthrough() {
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let upstream_forwarded = Arc::clone(&forwarded);
    // In live-hybrid mode `inventoryTransferCreate` hydrates its referenced locations
    // and inventory item from upstream before staging locally. That hydration node
    // query is the one legitimate forward in this scenario (the test clears it below);
    // answer it with two active locations and an item stocked at the origin so the
    // create passes validation. `inventoryTransferEdit`/`Duplicate` do not hydrate, so
    // any forward they make would be a real regression.
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            upstream_forwarded.lock().unwrap().push(request);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({"data": {"nodes": [
                    {
                        "__typename": "Location",
                        "id": "gid://shopify/Location/1",
                        "name": "Origin",
                        "isActive": true
                    },
                    {
                        "__typename": "Location",
                        "id": "gid://shopify/Location/2",
                        "name": "Destination",
                        "isActive": true
                    },
                    {
                        "__typename": "InventoryItem",
                        "id": "gid://shopify/InventoryItem/transfer-item",
                        "tracked": true,
                        "requiresShipping": true,
                        "variant": {
                            "id": "gid://shopify/ProductVariant/transfer-variant",
                            "title": "Transfer Variant",
                            "inventoryQuantity": 5,
                            "product": {
                                "id": "gid://shopify/Product/transfer-product",
                                "title": "Transfer Product",
                                "handle": "transfer-product",
                                "status": "ACTIVE",
                                "totalInventory": 5,
                                "tracksInventory": true
                            }
                        },
                        "inventoryLevels": {"nodes": [
                            {
                                "id": "gid://shopify/InventoryLevel/transfer-item-origin",
                                "location": {"id": "gid://shopify/Location/1", "name": "Origin"},
                                "quantities": [
                                    {"name": "available", "quantity": 5, "updatedAt": "2026-01-01T00:00:00Z"},
                                    {"name": "on_hand", "quantity": 5, "updatedAt": "2026-01-01T00:00:00Z"}
                                ]
                            }
                        ]}
                    }
                ]}}),
            }
        });

    let create_response = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/inventory-transfer-create.graphql"),
        json!({"input": {
            "originLocationId": "gid://shopify/Location/1",
            "destinationLocationId": "gid://shopify/Location/2",
            "lineItems": [{"inventoryItemId": "gid://shopify/InventoryItem/transfer-item", "quantity": 2}]
        }}),
    ));
    let transfer_id = create_response.body["data"]["inventoryTransferCreate"]["inventoryTransfer"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();
    forwarded.lock().unwrap().clear();

    let edit_response = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryTransferEditLocal($id: ID!, $input: InventoryTransferEditInput!) {
          inventoryTransferEdit(id: $id, input: $input) {
            inventoryTransfer {
              id
              status
              totalQuantity
              lineItems(first: 10) { nodes { inventoryItem { id } totalQuantity } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": transfer_id,
            "input": {
                "originId": "gid://shopify/Location/1",
                "destinationId": "gid://shopify/Location/2",
                "note": "Edited locally"
            }
        }),
    ));
    assert_eq!(
        edit_response.body["data"]["inventoryTransferEdit"]["inventoryTransfer"]["id"],
        json!(transfer_id)
    );
    assert_eq!(
        edit_response.body["data"]["inventoryTransferEdit"]["inventoryTransfer"]["totalQuantity"],
        json!(2)
    );
    assert_eq!(
        edit_response.body["data"]["inventoryTransferEdit"]["userErrors"],
        json!([])
    );

    let duplicate_response = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryTransferDuplicateLocal($id: ID!) {
          inventoryTransferDuplicate(id: $id) {
            inventoryTransfer {
              id
              status
              totalQuantity
              lineItems(first: 10) { nodes { inventoryItem { id } totalQuantity } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"id": transfer_id}),
    ));
    assert_ne!(
        duplicate_response.body["data"]["inventoryTransferDuplicate"]["inventoryTransfer"]["id"],
        json!(transfer_id)
    );
    assert_eq!(
        duplicate_response.body["data"]["inventoryTransferDuplicate"]["inventoryTransfer"]
            ["totalQuantity"],
        json!(2)
    );
    assert_eq!(
        duplicate_response.body["data"]["inventoryTransferDuplicate"]["userErrors"],
        json!([])
    );
    assert_eq!(forwarded.lock().unwrap().len(), 0);

    let roots: Vec<Value> = log_snapshot(&proxy)["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["interpreted"]["operationName"].clone())
        .collect();
    assert_eq!(
        roots,
        vec![
            json!("inventoryTransferCreate"),
            json!("inventoryTransferEdit"),
            json!("inventoryTransferDuplicate")
        ]
    );
}

#[test]
fn inventory_transfers_connection_filters_sorts_and_windows_staged_records() {
    let mut proxy = inventory_seed_proxy();
    let origin_location_id = add_inventory_test_location(&mut proxy, "Origin Stockroom");
    let destination_location_id = add_inventory_test_location(&mut proxy, "Destination Stockroom");
    let (_first_variant_id, first_item_id) =
        create_inventory_test_item(&mut proxy, "TRANSFER-ALPHA");
    let (second_variant_id, second_item_id) =
        create_inventory_test_item(&mut proxy, "TRANSFER-BETA");

    let seed_quantities = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedInventoryTransferStock($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            inventoryAdjustmentGroup { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"idempotencyKey": "inventory-transfer-connection-stock", "input": {"name": "available", "reason": "correction", "ignoreCompareQuantity": true, "quantities": [
            {"inventoryItemId": first_item_id, "locationId": origin_location_id, "quantity": 4},
            {"inventoryItemId": second_item_id, "locationId": origin_location_id, "quantity": 8}
        ]}}),
    ));
    assert_eq!(
        seed_quantities.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );

    let first_transfer = proxy.process_request(json_graphql_request(
        r#"
        mutation DraftInventoryTransfer($input: InventoryTransferCreateInput!) {
          inventoryTransferCreate(input: $input) {
            inventoryTransfer { id name status }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {
            "originLocationId": origin_location_id,
            "destinationLocationId": destination_location_id,
            "dateCreated": "2024-01-02T00:00:00Z",
            "tags": ["alpha"],
            "lineItems": [{"inventoryItemId": first_item_id, "quantity": 2}]
        }}),
    ));
    assert_eq!(
        first_transfer.body["data"]["inventoryTransferCreate"]["userErrors"],
        json!([])
    );
    let first_transfer_id = first_transfer.body["data"]["inventoryTransferCreate"]
        ["inventoryTransfer"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let second_transfer = proxy.process_request(json_graphql_request(
        r#"
        mutation ReadyInventoryTransfer($input: InventoryTransferCreateAsReadyToShipInput!) {
          inventoryTransferCreateAsReadyToShip(input: $input) {
            inventoryTransfer { id name status }
            userErrors { field message code }
          }
        }
        "#,
        json!({"input": {
            "originLocationId": origin_location_id,
            "destinationLocationId": destination_location_id,
            "dateCreated": "2024-01-03T00:00:00Z",
            "tags": ["beta"],
            "lineItems": [{"inventoryItemId": second_item_id, "quantity": 3}]
        }}),
    ));
    assert_eq!(
        second_transfer.body["data"]["inventoryTransferCreateAsReadyToShip"]["userErrors"],
        json!([])
    );
    let second_transfer_id = second_transfer.body["data"]["inventoryTransferCreateAsReadyToShip"]
        ["inventoryTransfer"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_transfer_name = second_transfer.body["data"]["inventoryTransferCreateAsReadyToShip"]
        ["inventoryTransfer"]["name"]
        .as_str()
        .unwrap()
        .to_string();
    let origin_location_tail = origin_location_id
        .rsplit('/')
        .next()
        .and_then(|tail| tail.split('?').next())
        .unwrap()
        .to_string();
    let second_item_tail = second_item_id
        .rsplit('/')
        .next()
        .and_then(|tail| tail.split('?').next())
        .unwrap()
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query InventoryTransfersConnection($after: String, $readyQuery: String!, $nameQuery: String!, $variantQuery: String!, $originTailQuery: String!, $itemTailQuery: String!) {
          firstPage: inventoryTransfers(first: 1, sortKey: NAME) {
            nodes { id name status totalQuantity }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          secondPage: inventoryTransfers(first: 1, after: $after, sortKey: NAME) {
            nodes { id name status totalQuantity }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          reversePage: inventoryTransfers(first: 1, sortKey: NAME, reverse: true) {
            nodes { id name status totalQuantity }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          readyOnly: inventoryTransfers(first: 10, query: $readyQuery, sortKey: NAME) {
            nodes { id name status totalQuantity }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          byName: inventoryTransfers(first: 10, query: $nameQuery) {
            nodes { id name }
          }
          byVariant: inventoryTransfers(first: 10, query: $variantQuery) {
            nodes { id name }
          }
          byOriginTail: inventoryTransfers(first: 10, query: $originTailQuery, sortKey: NAME) {
            nodes { id name status }
          }
          byInventoryItemTail: inventoryTransfers(first: 10, query: $itemTailQuery) {
            nodes { id name }
          }
          unknownFilter: inventoryTransfers(first: 10, query: "unknown_field:anything") {
            nodes { id }
          }
        }
        "#,
        json!({
            "after": first_transfer_id,
            "readyQuery": "status:READY_TO_SHIP tag:beta",
            "nameQuery": second_transfer_name,
            "variantQuery": format!("product_variant_id:{second_variant_id}"),
            "originTailQuery": format!("origin_id:{origin_location_tail}"),
            "itemTailQuery": format!("inventory_item_id:{second_item_tail}")
        }),
    ));

    assert_eq!(
        read.body["data"]["firstPage"],
        json!({
            "nodes": [{"id": first_transfer_id, "name": "#T0001", "status": "DRAFT", "totalQuantity": 2}],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": first_transfer_id,
                "endCursor": first_transfer_id
            }
        })
    );
    assert_eq!(
        read.body["data"]["secondPage"]["nodes"],
        json!([{"id": second_transfer_id, "name": "#T0002", "status": "READY_TO_SHIP", "totalQuantity": 3}])
    );
    assert_eq!(
        read.body["data"]["secondPage"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );
    assert_eq!(
        read.body["data"]["reversePage"]["nodes"],
        json!([{"id": second_transfer_id, "name": "#T0002", "status": "READY_TO_SHIP", "totalQuantity": 3}])
    );
    assert_eq!(
        read.body["data"]["readyOnly"]["nodes"],
        json!([{"id": second_transfer_id, "name": "#T0002", "status": "READY_TO_SHIP", "totalQuantity": 3}])
    );
    assert_eq!(
        read.body["data"]["readyOnly"]["pageInfo"],
        json!({"hasNextPage": false, "hasPreviousPage": false, "startCursor": second_transfer_id, "endCursor": second_transfer_id})
    );
    assert_eq!(
        read.body["data"]["byName"]["nodes"],
        json!([{ "id": second_transfer_id, "name": "#T0002" }])
    );
    assert_eq!(
        read.body["data"]["byVariant"]["nodes"],
        json!([{ "id": second_transfer_id, "name": "#T0002" }])
    );
    assert_eq!(
        read.body["data"]["byOriginTail"]["nodes"],
        json!([
            {"id": first_transfer_id, "name": "#T0001", "status": "DRAFT"},
            {"id": second_transfer_id, "name": "#T0002", "status": "READY_TO_SHIP"}
        ])
    );
    assert_eq!(
        read.body["data"]["byInventoryItemTail"]["nodes"],
        json!([{ "id": second_transfer_id, "name": "#T0002" }])
    );
    assert_eq!(read.body["data"]["unknownFilter"]["nodes"], json!([]));
}

#[test]
fn combined_listing_product_create_preserves_captured_parent_roles() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/combinedListingUpdate-validation.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    // The proxy fabricates products it never saw upstream, so it mints a
    // proxy-synthetic id rather than replaying the captured real product id.
    // Every other captured field (role, handle, title, userErrors) must still
    // match the fixture verbatim — only the id is rewritten to whatever
    // synthetic gid the engine allocated for this call (the value depends on how
    // many log ids the operation reserved, so we read it back rather than
    // hardcode the counter).
    for operation_key in ["createParentAlready", "createParentEditRemove"] {
        let response = proxy.process_request(json_graphql_request(
            include_str!(
                "../../config/parity-requests/products/combinedListingUpdate-validation-product-create.graphql"
            ),
            fixture["operations"][operation_key]["request"]["variables"].clone(),
        ));
        let actual_id = response.body["data"]["productCreate"]["product"]["id"]
            .as_str()
            .unwrap_or_default();
        assert!(
            actual_id.starts_with("gid://shopify/Product/")
                && actual_id.ends_with("?shopify-draft-proxy=synthetic"),
            "combined listing productCreate {operation_key} should mint a synthetic product id, got {actual_id:?}"
        );
        let mut expected = fixture["operations"][operation_key]["response"]["data"].clone();
        expected["productCreate"]["product"]["id"] = json!(actual_id);
        assert_eq!(
            response.body["data"], expected,
            "combined listing productCreate {operation_key} should preserve requested parent role"
        );
    }
}

#[test]
fn online_store_mobile_platform_application_lifecycle_and_validation_are_local() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MobilePlatformApplicationUpdateCreate {
          appleCreate: mobilePlatformApplicationCreate(input: { apple: { appId: "com.example.apple.old", universalLinksEnabled: false, sharedWebCredentialsEnabled: true, appClipsEnabled: false, appClipApplicationId: "com.example.apple.old.Clip" } }) {
            mobilePlatformApplication { __typename ... on AppleApplication { id appId universalLinksEnabled sharedWebCredentialsEnabled appClipsEnabled appClipApplicationId } }
            userErrors { code field message }
          }
          androidCreate: mobilePlatformApplicationCreate(input: { android: { applicationId: "com.example.android.old", appLinksEnabled: false, sha256CertFingerprints: ["AA:BB"] } }) {
            mobilePlatformApplication { __typename ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints } }
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    let apple_id = create.body["data"]["appleCreate"]["mobilePlatformApplication"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let android_id = create.body["data"]["androidCreate"]["mobilePlatformApplication"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"],
        json!({
            "appleCreate": {"mobilePlatformApplication": {"__typename": "AppleApplication", "id": apple_id, "appId": "com.example.apple.old", "universalLinksEnabled": false, "sharedWebCredentialsEnabled": true, "appClipsEnabled": false, "appClipApplicationId": "com.example.apple.old.Clip"}, "userErrors": []},
            "androidCreate": {"mobilePlatformApplication": {"__typename": "AndroidApplication", "id": android_id, "applicationId": "com.example.android.old", "appLinksEnabled": false, "sha256CertFingerprints": ["AA:BB"]}, "userErrors": []}
        })
    );

    let apple_update = proxy.process_request(json_graphql_request(
        r#"
        mutation MobilePlatformApplicationUpdateApple($id: ID!) {
          mobilePlatformApplicationUpdate(id: $id, input: { apple: { appId: "com.example.apple.new", universalLinksEnabled: true, sharedWebCredentialsEnabled: false, appClipsEnabled: true, appClipApplicationId: "com.example.apple.new.Clip" } }) {
            mobilePlatformApplication { __typename ... on AppleApplication { id appId universalLinksEnabled sharedWebCredentialsEnabled appClipsEnabled appClipApplicationId } }
            userErrors { code field message }
          }
        }
        "#,
        json!({"id": apple_id}),
    ));
    assert_eq!(
        apple_update.body["data"]["mobilePlatformApplicationUpdate"]["mobilePlatformApplication"]
            ["appId"],
        json!("com.example.apple.new")
    );
    assert_eq!(
        apple_update.body["data"]["mobilePlatformApplicationUpdate"]["userErrors"],
        json!([])
    );

    let android_update = proxy.process_request(json_graphql_request(
        r#"
        mutation MobilePlatformApplicationUpdateAndroid($id: ID!) {
          mobilePlatformApplicationUpdate(id: $id, input: { android: { applicationId: "com.example.android.new", appLinksEnabled: true, sha256CertFingerprints: ["CC:DD", "EE:FF"] } }) {
            mobilePlatformApplication { __typename ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints } }
            userErrors { code field message }
          }
        }
        "#,
        json!({"id": android_id}),
    ));
    assert_eq!(
        android_update.body["data"]["mobilePlatformApplicationUpdate"]["mobilePlatformApplication"]
            ["applicationId"],
        json!("com.example.android.new")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MobilePlatformApplicationUpdateReadAfterValidation($appleId: ID!, $androidId: ID!) {
          apple: mobilePlatformApplication(id: $appleId) { __typename ... on AppleApplication { id appId universalLinksEnabled sharedWebCredentialsEnabled appClipsEnabled appClipApplicationId } }
          android: mobilePlatformApplication(id: $androidId) { __typename ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints } }
        }
        "#,
        json!({"appleId": apple_id, "androidId": android_id}),
    ));
    assert_eq!(
        read.body["data"]["apple"]["appId"],
        json!("com.example.apple.new")
    );
    assert_eq!(
        read.body["data"]["android"]["sha256CertFingerprints"],
        json!(["CC:DD", "EE:FF"])
    );

    let validation = proxy.process_request(json_graphql_request(
        r#"
        mutation MobilePlatformApplicationUpdateValidation($appleId: ID!, $androidId: ID!, $missingId: ID!) {
          platformMismatch: mobilePlatformApplicationUpdate(id: $androidId, input: { apple: { appId: "com.example.wrong-platform" } }) { mobilePlatformApplication { __typename } userErrors { code field message } }
          missing: mobilePlatformApplicationUpdate(id: $missingId, input: { apple: { appId: "com.example.missing" } }) { mobilePlatformApplication { __typename } userErrors { code field message } }
          blankAndroid: mobilePlatformApplicationUpdate(id: $androidId, input: { android: { applicationId: "" } }) { mobilePlatformApplication { __typename } userErrors { code field message } }
          blankApple: mobilePlatformApplicationUpdate(id: $appleId, input: { apple: { appId: "  " } }) { mobilePlatformApplication { __typename } userErrors { code field message } }
        }
        "#,
        json!({"appleId": apple_id, "androidId": android_id, "missingId": "gid://shopify/MobilePlatformApplication/9999999999"}),
    ));
    assert_eq!(
        validation.body["data"]["platformMismatch"]["userErrors"][0]["code"],
        json!("INVALID")
    );
    assert_eq!(
        validation.body["data"]["missing"]["userErrors"][0]["code"],
        json!("NOT_FOUND")
    );
    assert_eq!(
        validation.body["data"]["blankAndroid"]["userErrors"][0]["code"],
        json!("BLANK")
    );
    assert_eq!(
        validation.body["data"]["blankApple"]["userErrors"][0]["code"],
        json!("BLANK")
    );
}

#[test]
fn online_store_mobile_platform_application_create_accepts_repeated_platforms() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MobilePlatformApplicationRepeatedCreates {
          androidOne: mobilePlatformApplicationCreate(input: { android: { applicationId: "com.example.android.one", appLinksEnabled: true, sha256CertFingerprints: ["AA:BB"] } }) {
            mobilePlatformApplication { __typename ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints } }
            userErrors { code field message }
          }
          androidTwo: mobilePlatformApplicationCreate(input: { android: { applicationId: "com.example.android.two", appLinksEnabled: false, sha256CertFingerprints: ["CC:DD"] } }) {
            mobilePlatformApplication { __typename ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints } }
            userErrors { code field message }
          }
          appleOne: mobilePlatformApplicationCreate(input: { apple: { appId: "com.example.apple.one", universalLinksEnabled: false, sharedWebCredentialsEnabled: false, appClipsEnabled: false } }) {
            mobilePlatformApplication { __typename ... on AppleApplication { id appId universalLinksEnabled sharedWebCredentialsEnabled appClipsEnabled appClipApplicationId } }
            userErrors { code field message }
          }
          appleTwo: mobilePlatformApplicationCreate(input: { apple: { appId: "com.example.apple.two", universalLinksEnabled: true, sharedWebCredentialsEnabled: true, appClipsEnabled: false } }) {
            mobilePlatformApplication { __typename ... on AppleApplication { id appId universalLinksEnabled sharedWebCredentialsEnabled appClipsEnabled appClipApplicationId } }
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(create.body["data"]["androidOne"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["androidTwo"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["appleOne"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["appleTwo"]["userErrors"], json!([]));

    let android_one_id = create.body["data"]["androidOne"]["mobilePlatformApplication"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let android_two_id = create.body["data"]["androidTwo"]["mobilePlatformApplication"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let apple_one_id = create.body["data"]["appleOne"]["mobilePlatformApplication"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let apple_two_id = create.body["data"]["appleTwo"]["mobilePlatformApplication"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(android_one_id, android_two_id);
    assert_ne!(apple_one_id, apple_two_id);

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MobilePlatformApplicationRepeatedCreatesRead {
          mobilePlatformApplications(first: 10) {
            nodes {
              __typename
              ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints }
              ... on AppleApplication { id appId universalLinksEnabled sharedWebCredentialsEnabled appClipsEnabled appClipApplicationId }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["mobilePlatformApplications"]["nodes"],
        json!([
            {"__typename": "AndroidApplication", "id": android_one_id, "applicationId": "com.example.android.one", "appLinksEnabled": true, "sha256CertFingerprints": ["AA:BB"]},
            {"__typename": "AndroidApplication", "id": android_two_id, "applicationId": "com.example.android.two", "appLinksEnabled": false, "sha256CertFingerprints": ["CC:DD"]},
            {"__typename": "AppleApplication", "id": apple_one_id, "appId": "com.example.apple.one", "universalLinksEnabled": false, "sharedWebCredentialsEnabled": false, "appClipsEnabled": false, "appClipApplicationId": ""},
            {"__typename": "AppleApplication", "id": apple_two_id, "appId": "com.example.apple.two", "universalLinksEnabled": true, "sharedWebCredentialsEnabled": true, "appClipsEnabled": false, "appClipApplicationId": ""}
        ])
    );
}

#[test]
fn mobile_platform_applications_connection_paginates_edges_nodes_and_page_info_consistently() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MobilePlatformApplicationUpdateCreate {
          appleOne: mobilePlatformApplicationCreate(input: { apple: { appId: "com.example.apple.one", universalLinksEnabled: false, sharedWebCredentialsEnabled: false, appClipsEnabled: false } }) {
            mobilePlatformApplication { __typename ... on AppleApplication { id appId } }
            userErrors { code field message }
          }
          android: mobilePlatformApplicationCreate(input: { android: { applicationId: "com.example.android", appLinksEnabled: true, sha256CertFingerprints: ["AA:BB"] } }) {
            mobilePlatformApplication { __typename ... on AndroidApplication { id applicationId } }
            userErrors { code field message }
          }
          appleTwo: mobilePlatformApplicationCreate(input: { apple: { appId: "com.example.apple.two", universalLinksEnabled: true, sharedWebCredentialsEnabled: true, appClipsEnabled: false } }) {
            mobilePlatformApplication { __typename ... on AppleApplication { id appId } }
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(create.body["data"]["appleOne"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["android"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["appleTwo"]["userErrors"], json!([]));

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query MobilePlatformApplicationUpdateReadAfterValidation($first: Int!) {
          mobilePlatformApplications(first: $first) {
            nodes { __typename ... on AppleApplication { id appId } ... on AndroidApplication { id applicationId } }
            edges { cursor node { __typename ... on AppleApplication { id appId } ... on AndroidApplication { id applicationId } } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"first": 1}),
    ));
    assert_eq!(
        first_page.body["data"]["mobilePlatformApplications"],
        json!({
            "nodes": [
                {"__typename": "AppleApplication", "id": "gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic", "appId": "com.example.apple.one"}
            ],
            "edges": [
                {"cursor": "gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic", "node": {"__typename": "AppleApplication", "id": "gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic", "appId": "com.example.apple.one"}}
            ],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": "gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic",
                "endCursor": "gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic"
            }
        })
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query MobilePlatformApplicationUpdateReadAfterValidation($first: Int!, $after: String!) {
          mobilePlatformApplications(first: $first, after: $after) {
            nodes { __typename ... on AppleApplication { id appId } ... on AndroidApplication { id applicationId } }
            edges { cursor node { __typename ... on AppleApplication { id appId } ... on AndroidApplication { id applicationId } } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"first": 1, "after": first_page.body["data"]["mobilePlatformApplications"]["pageInfo"]["endCursor"]}),
    ));
    assert_eq!(
        second_page.body["data"]["mobilePlatformApplications"],
        json!({
            "nodes": [{"__typename": "AndroidApplication", "id": "gid://shopify/MobilePlatformApplication/2?shopify-draft-proxy=synthetic", "applicationId": "com.example.android"}],
            "edges": [{"cursor": "gid://shopify/MobilePlatformApplication/2?shopify-draft-proxy=synthetic", "node": {"__typename": "AndroidApplication", "id": "gid://shopify/MobilePlatformApplication/2?shopify-draft-proxy=synthetic", "applicationId": "com.example.android"}}],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": true,
                "startCursor": "gid://shopify/MobilePlatformApplication/2?shopify-draft-proxy=synthetic",
                "endCursor": "gid://shopify/MobilePlatformApplication/2?shopify-draft-proxy=synthetic"
            }
        })
    );

    let third_page = proxy.process_request(json_graphql_request(
        r#"
        query MobilePlatformApplicationUpdateReadAfterValidation($first: Int!, $after: String!) {
          mobilePlatformApplications(first: $first, after: $after) {
            nodes { __typename ... on AppleApplication { id appId } ... on AndroidApplication { id applicationId } }
            edges { cursor node { __typename ... on AppleApplication { id appId } ... on AndroidApplication { id applicationId } } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"first": 1, "after": second_page.body["data"]["mobilePlatformApplications"]["pageInfo"]["endCursor"]}),
    ));
    assert_eq!(
        third_page.body["data"]["mobilePlatformApplications"],
        json!({
            "nodes": [{"__typename": "AppleApplication", "id": "gid://shopify/MobilePlatformApplication/3?shopify-draft-proxy=synthetic", "appId": "com.example.apple.two"}],
            "edges": [{"cursor": "gid://shopify/MobilePlatformApplication/3?shopify-draft-proxy=synthetic", "node": {"__typename": "AppleApplication", "id": "gid://shopify/MobilePlatformApplication/3?shopify-draft-proxy=synthetic", "appId": "com.example.apple.two"}}],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": "gid://shopify/MobilePlatformApplication/3?shopify-draft-proxy=synthetic",
                "endCursor": "gid://shopify/MobilePlatformApplication/3?shopify-draft-proxy=synthetic"
            }
        })
    );
}

#[test]
fn online_store_mobile_platform_application_create_model_validations_do_not_stage() {
    let mut proxy = snapshot_proxy();
    let long_application_id = "a".repeat(101);
    let long_app_clip_application_id = "c".repeat(256);

    let validation = proxy.process_request(json_graphql_request(
        r#"
        mutation MobilePlatformApplicationCreateModelValidation($longApplicationId: String!, $longAppClipApplicationId: String!) {
          longAndroid: mobilePlatformApplicationCreate(input: { android: { applicationId: $longApplicationId, appLinksEnabled: true, sha256CertFingerprints: ["AA:BB"] } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
          missingAndroidFingerprints: mobilePlatformApplicationCreate(input: { android: { applicationId: "com.example.missing.fingerprints", appLinksEnabled: true } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
          emptyAndroidFingerprints: mobilePlatformApplicationCreate(input: { android: { applicationId: "com.example.empty.fingerprints", appLinksEnabled: true, sha256CertFingerprints: [] } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
          longApple: mobilePlatformApplicationCreate(input: { apple: { appId: $longApplicationId, universalLinksEnabled: false, sharedWebCredentialsEnabled: false, appClipsEnabled: false } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
          missingAppClip: mobilePlatformApplicationCreate(input: { apple: { appId: "com.example.clip", universalLinksEnabled: false, sharedWebCredentialsEnabled: false, appClipsEnabled: true } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
          longAppClip: mobilePlatformApplicationCreate(input: { apple: { appId: "com.example.clip.long", universalLinksEnabled: false, sharedWebCredentialsEnabled: false, appClipsEnabled: true, appClipApplicationId: $longAppClipApplicationId } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
        }
        "#,
        json!({
            "longApplicationId": long_application_id,
            "longAppClipApplicationId": long_app_clip_application_id
        }),
    ));

    assert_eq!(
        validation.body["data"],
        json!({
            "longAndroid": {"mobilePlatformApplication": null, "userErrors": [{"code": "TOO_LONG", "field": ["input", "android", "applicationId"], "message": "Application ID is too long (maximum is 100 characters)"}]},
            "missingAndroidFingerprints": {"mobilePlatformApplication": null, "userErrors": [{"code": "BLANK", "field": ["input", "android", "sha256CertFingerprints"], "message": "Sha256 cert fingerprints can't be blank"}]},
            "emptyAndroidFingerprints": {"mobilePlatformApplication": null, "userErrors": [{"code": "BLANK", "field": ["input", "android", "sha256CertFingerprints"], "message": "Sha256 cert fingerprints can't be blank"}]},
            "longApple": {"mobilePlatformApplication": null, "userErrors": [{"code": "TOO_LONG", "field": ["input", "apple", "appId"], "message": "Application ID is too long (maximum is 100 characters)"}]},
            "missingAppClip": {"mobilePlatformApplication": null, "userErrors": [{"code": "BLANK", "field": ["input", "apple", "appClipApplicationId"], "message": "App clip application can't be blank"}]},
            "longAppClip": {"mobilePlatformApplication": null, "userErrors": [{"code": "TOO_LONG", "field": ["input", "apple", "appClipApplicationId"], "message": "App clip application is too long (maximum is 255 characters)"}]}
        })
    );
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
}

#[test]
fn online_store_mobile_platform_application_update_model_validations_do_not_mutate() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MobilePlatformApplicationUpdateCreate {
          appleCreate: mobilePlatformApplicationCreate(input: { apple: { appId: "com.example.apple.old", universalLinksEnabled: false, sharedWebCredentialsEnabled: true, appClipsEnabled: false } }) {
            mobilePlatformApplication { __typename ... on AppleApplication { id appId universalLinksEnabled sharedWebCredentialsEnabled appClipsEnabled appClipApplicationId } }
            userErrors { code field message }
          }
          androidCreate: mobilePlatformApplicationCreate(input: { android: { applicationId: "com.example.android.old", appLinksEnabled: false, sha256CertFingerprints: ["AA:BB"] } }) {
            mobilePlatformApplication { __typename ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints } }
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    let apple_id = create.body["data"]["appleCreate"]["mobilePlatformApplication"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let android_id = create.body["data"]["androidCreate"]["mobilePlatformApplication"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 1);

    let long_application_id = "a".repeat(101);
    let long_app_clip_application_id = "c".repeat(256);
    let validation = proxy.process_request(json_graphql_request(
        r#"
        mutation MobilePlatformApplicationUpdateModelValidation($appleId: ID!, $androidId: ID!, $longApplicationId: String!, $longAppClipApplicationId: String!) {
          longAndroid: mobilePlatformApplicationUpdate(id: $androidId, input: { android: { applicationId: $longApplicationId, appLinksEnabled: true, sha256CertFingerprints: ["AA:BB"] } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
          missingAndroidFingerprints: mobilePlatformApplicationUpdate(id: $androidId, input: { android: { applicationId: "com.example.android.missing", appLinksEnabled: true } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
          emptyAndroidFingerprints: mobilePlatformApplicationUpdate(id: $androidId, input: { android: { applicationId: "com.example.android.new", appLinksEnabled: true, sha256CertFingerprints: [] } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
          longApple: mobilePlatformApplicationUpdate(id: $appleId, input: { apple: { appId: $longApplicationId, universalLinksEnabled: true, sharedWebCredentialsEnabled: false, appClipsEnabled: false } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
          missingAppClip: mobilePlatformApplicationUpdate(id: $appleId, input: { apple: { appId: "com.example.apple.clip", universalLinksEnabled: true, sharedWebCredentialsEnabled: false, appClipsEnabled: true } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
          longAppClip: mobilePlatformApplicationUpdate(id: $appleId, input: { apple: { appId: "com.example.apple.clip.long", universalLinksEnabled: true, sharedWebCredentialsEnabled: false, appClipsEnabled: true, appClipApplicationId: $longAppClipApplicationId } }) {
            mobilePlatformApplication { __typename }
            userErrors { code field message }
          }
        }
        "#,
        json!({
            "appleId": apple_id,
            "androidId": android_id,
            "longApplicationId": long_application_id,
            "longAppClipApplicationId": long_app_clip_application_id
        }),
    ));

    assert_eq!(
        validation.body["data"],
        json!({
            "longAndroid": {"mobilePlatformApplication": null, "userErrors": [{"code": "TOO_LONG", "field": ["input", "android", "applicationId"], "message": "Application ID is too long (maximum is 100 characters)"}]},
            "missingAndroidFingerprints": {"mobilePlatformApplication": null, "userErrors": [{"code": "BLANK", "field": ["input", "android", "sha256CertFingerprints"], "message": "Sha256 cert fingerprints can't be blank"}]},
            "emptyAndroidFingerprints": {"mobilePlatformApplication": null, "userErrors": [{"code": "BLANK", "field": ["input", "android", "sha256CertFingerprints"], "message": "Sha256 cert fingerprints can't be blank"}]},
            "longApple": {"mobilePlatformApplication": null, "userErrors": [{"code": "TOO_LONG", "field": ["input", "apple", "appId"], "message": "Application ID is too long (maximum is 100 characters)"}]},
            "missingAppClip": {"mobilePlatformApplication": null, "userErrors": [{"code": "BLANK", "field": ["input", "apple", "appClipApplicationId"], "message": "App clip application can't be blank"}]},
            "longAppClip": {"mobilePlatformApplication": null, "userErrors": [{"code": "TOO_LONG", "field": ["input", "apple", "appClipApplicationId"], "message": "App clip application is too long (maximum is 255 characters)"}]}
        })
    );
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 1);

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MobilePlatformApplicationUpdateReadAfterValidation($appleId: ID!, $androidId: ID!) {
          apple: mobilePlatformApplication(id: $appleId) { __typename ... on AppleApplication { id appId universalLinksEnabled sharedWebCredentialsEnabled appClipsEnabled appClipApplicationId } }
          android: mobilePlatformApplication(id: $androidId) { __typename ... on AndroidApplication { id applicationId appLinksEnabled sha256CertFingerprints } }
        }
        "#,
        json!({"appleId": apple_id, "androidId": android_id}),
    ));
    assert_eq!(
        read.body["data"]["apple"],
        json!({"__typename": "AppleApplication", "id": apple_id, "appId": "com.example.apple.old", "universalLinksEnabled": false, "sharedWebCredentialsEnabled": true, "appClipsEnabled": false, "appClipApplicationId": ""})
    );
    assert_eq!(
        read.body["data"]["android"],
        json!({"__typename": "AndroidApplication", "id": android_id, "applicationId": "com.example.android.old", "appLinksEnabled": false, "sha256CertFingerprints": ["AA:BB"]})
    );
}

#[test]
fn online_store_script_tag_web_pixel_and_theme_file_validation_are_local() {
    let mut proxy = snapshot_proxy();

    let script_validation = proxy.process_request(json_graphql_request(
        r#"
        mutation ScriptTagCreateValidatesSrc {
          blank: scriptTagCreate(input: { src: "" }) { scriptTag { id src displayScope } userErrors {  field message } }
          tooLong: scriptTagCreate(input: { src: "https://example.test/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }) { scriptTag { id src displayScope } userErrors {  field message } }
          invalid: scriptTagCreate(input: { src: "not-a-url" }) { scriptTag { id src displayScope } userErrors {  field message } }
          http: scriptTagCreate(input: { src: "http://example.test/app.js" }) { scriptTag { id src displayScope } userErrors {  field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        script_validation.body["data"]["blank"]["userErrors"][0],
        json!({"field": ["input", "src"], "message": "Source can't be blank"})
    );
    assert_eq!(
        script_validation.body["data"]["tooLong"]["userErrors"][0]["message"],
        json!("Source is too long (maximum is 255 characters)")
    );
    assert_eq!(
        script_validation.body["data"]["invalid"]["userErrors"][0]["message"],
        json!("Source is invalid")
    );
    assert_eq!(
        script_validation.body["data"]["http"]["userErrors"][0]["message"],
        json!("Source is invalid")
    );

    let create_script = proxy.process_request(json_graphql_request(
        r#"
        mutation ScriptTagUpdateValidationCreate {
          scriptTagCreate(input: { src: "https://cdn.example.test/app.js", displayScope: ALL }) { scriptTag { id src displayScope event cache } userErrors {  field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        create_script.body["data"]["scriptTagCreate"]["scriptTag"],
        json!({"id": "gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic", "src": "https://cdn.example.test/app.js", "displayScope": "ALL", "event": "onload", "cache": false})
    );

    let script_update = proxy.process_request(json_graphql_request(
        r#"
        mutation ScriptTagUpdateEventForceOnload {
          scriptTagUpdate(id: "gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic", input: { event: "onstart", cache: true }) { scriptTag { id src displayScope event cache } userErrors {  field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        script_update.body["data"]["scriptTagUpdate"]["scriptTag"]["event"],
        json!("onload")
    );
    assert_eq!(
        script_update.body["data"]["scriptTagUpdate"]["scriptTag"]["cache"],
        json!(true)
    );
    let script_update_log_len = log_snapshot(&proxy)["entries"].as_array().unwrap().len();

    let invalid_script_updates = proxy.process_request(json_graphql_request(
        r#"
        mutation ScriptTagUpdateValidatesChangedSrc($longSrc: String!) {
          blank: scriptTagUpdate(id: "gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic", input: { src: "   " }) { scriptTag { id src } userErrors {  field message } }
          tooLong: scriptTagUpdate(id: "gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic", input: { src: $longSrc }) { scriptTag { id src } userErrors {  field message } }
          invalid: scriptTagUpdate(id: "gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic", input: { src: "not-a-url" }) { scriptTag { id src } userErrors {  field message } }
          http: scriptTagUpdate(id: "gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic", input: { src: "http://example.test/app.js" }) { scriptTag { id src } userErrors {  field message } }
          badScope: scriptTagUpdate(id: "gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic", input: { displayScope: STOREFRONT }) { scriptTag { id displayScope } userErrors {  field message } }
        }
        "#,
        json!({"longSrc": format!("https://example.test/{}", "a".repeat(260))}),
    ));
    assert_eq!(
        invalid_script_updates.body["data"]["blank"],
        json!({"scriptTag": null, "userErrors": [{"field": ["src"], "message": "Source can't be blank"}]})
    );
    assert_eq!(
        invalid_script_updates.body["data"]["tooLong"],
        json!({"scriptTag": null, "userErrors": [{"field": ["src"], "message": "Source is too long (maximum is 255 characters)"}]})
    );
    assert_eq!(
        invalid_script_updates.body["data"]["invalid"],
        json!({"scriptTag": null, "userErrors": [{"field": ["src"], "message": "Source is invalid"}]})
    );
    assert_eq!(
        invalid_script_updates.body["data"]["http"],
        json!({"scriptTag": null, "userErrors": [{"field": ["src"], "message": "Source is invalid"}]})
    );
    assert_eq!(
        invalid_script_updates.body["data"]["badScope"],
        json!({"scriptTag": null, "userErrors": [{"field": ["displayScope"], "message": "Display scope is not included in the list"}]})
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        script_update_log_len
    );

    let script_read_after_invalid_update = proxy.process_request(json_graphql_request(
        r#"
        query ScriptTagReadAfterInvalidUpdate {
          scriptTag(id: "gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic") { id src displayScope event cache }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        script_read_after_invalid_update.body["data"]["scriptTag"],
        json!({"id": "gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic", "src": "https://cdn.example.test/app.js", "displayScope": "ALL", "event": "onload", "cache": true})
    );

    let web_pixel = proxy.process_request(json_graphql_request(
        r#"
        mutation WebPixelUpdateValidationLocalRuntime {
          create: webPixelCreate(webPixel: {}) { webPixel { id status settings } userErrors { __typename code field message } }
          invalidJson: webPixelUpdate(id: "gid://shopify/WebPixel/2?shopify-draft-proxy=synthetic", webPixel: { settings: "not json" }) { webPixel { id settings status } userErrors { __typename code field message } }
          validUpdate: webPixelUpdate(id: "gid://shopify/WebPixel/2?shopify-draft-proxy=synthetic", webPixel: { settings: "{\"accountID\":\"abc\"}" }) { webPixel { id settings status } userErrors { __typename code field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        web_pixel.body["data"]["invalidJson"]["userErrors"][0]["code"],
        json!("INVALID_CONFIGURATION_JSON")
    );
    assert_eq!(
        web_pixel.body["data"]["validUpdate"]["webPixel"]["settings"],
        json!({"accountID": "abc"})
    );

    let theme_files = proxy.process_request(json_graphql_request(
        r#"
        mutation ThemeFilesChecksumsAndValidation {
          themeCreate(source: "https://example.com/har-585-theme.zip", name: "HAR 585 theme", role: UNPUBLISHED) { theme { id } userErrors { field message code } }
          first: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "templates/index.json", body: { type: TEXT, value: "hello" } }]) { upsertedThemeFiles { filename checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } }
          second: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "templates/index.json", body: { type: TEXT, value: "hello world" } }]) { upsertedThemeFiles { filename checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } }
          invalid: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "evil/path.liquid", body: { type: TEXT, value: "ignored" } }]) { upsertedThemeFiles { filename } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        theme_files.body["data"]["first"]["upsertedThemeFiles"][0]["checksumMd5"],
        json!("5d41402abc4b2a76b9719d911017c592")
    );
    assert_eq!(
        theme_files.body["data"]["second"]["upsertedThemeFiles"][0]["size"],
        json!(11)
    );
    assert_eq!(
        theme_files.body["data"]["invalid"]["userErrors"][0]["code"],
        json!("INVALID")
    );
}

#[test]
fn online_store_script_tag_root_dispatch_delete_and_not_found_are_local() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DeliberatelyNotAScriptTagOperationName {
          first: scriptTagCreate(input: { src: "https://cdn.example.test/first.js", displayScope: ALL }) { scriptTag { id src displayScope event cache } userErrors {  field message } }
          second: scriptTagCreate(input: { src: "https://cdn.example.test/second.js", displayScope: ORDER_STATUS, cache: true }) { scriptTag { id src displayScope event cache } userErrors {  field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(create.body["data"]["first"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["second"]["userErrors"], json!([]));
    let first_id = create.body["data"]["first"]["scriptTag"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_id = create.body["data"]["second"]["scriptTag"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadStagedScriptTags($firstId: ID!) {
          first: scriptTag(id: $firstId) { id src displayScope event cache }
          scriptTags(first: 10) {
            nodes { id src displayScope event cache }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"firstId": first_id}),
    ));
    assert_eq!(
        read.body["data"]["first"],
        json!({"id": first_id, "src": "https://cdn.example.test/first.js", "displayScope": "ALL", "event": "onload", "cache": false})
    );
    assert_eq!(
        read.body["data"]["scriptTags"],
        json!({
            "nodes": [
                {"id": first_id, "src": "https://cdn.example.test/first.js", "displayScope": "ALL", "event": "onload", "cache": false},
                {"id": second_id, "src": "https://cdn.example.test/second.js", "displayScope": "ORDER_STATUS", "event": "onload", "cache": true}
            ],
            "edges": [
                {"cursor": first_id, "node": {"id": first_id}},
                {"cursor": second_id, "node": {"id": second_id}}
            ],
            "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": first_id, "endCursor": second_id}
        })
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteStagedScriptTags($firstId: ID!, $missingId: ID!) {
          deleteFirst: scriptTagDelete(id: $firstId) { deletedScriptTagId userErrors { __typename  field message } }
          deleteMissing: scriptTagDelete(id: $missingId) { deletedScriptTagId userErrors { __typename  field message } }
        }
        "#,
        json!({
            "firstId": first_id,
            "missingId": "gid://shopify/ScriptTag/999999?shopify-draft-proxy=synthetic"
        }),
    ));
    assert_eq!(
        delete.body["data"]["deleteFirst"],
        json!({"deletedScriptTagId": first_id, "userErrors": []})
    );
    assert_eq!(
        delete.body["data"]["deleteMissing"],
        json!({"deletedScriptTagId": null, "userErrors": [{
            "__typename": "ScriptTagUserError",
            "field": ["id"],
            "message": "Script tag not found"
        }]})
    );

    let read_after_delete = proxy.process_request(json_graphql_request(
        r#"
        query ReadScriptTagsAfterDelete($firstId: ID!, $secondId: ID!) {
          deleted: scriptTag(id: $firstId) { id }
          kept: scriptTag(id: $secondId) { id src displayScope event cache }
          scriptTags(first: 10) { nodes { id src displayScope event cache } }
        }
        "#,
        json!({"firstId": first_id, "secondId": second_id}),
    ));
    assert_eq!(read_after_delete.body["data"]["deleted"], Value::Null);
    assert_eq!(
        read_after_delete.body["data"]["kept"],
        json!({"id": second_id, "src": "https://cdn.example.test/second.js", "displayScope": "ORDER_STATUS", "event": "onload", "cache": true})
    );
    assert_eq!(
        read_after_delete.body["data"]["scriptTags"]["nodes"],
        json!([{"id": second_id, "src": "https://cdn.example.test/second.js", "displayScope": "ORDER_STATUS", "event": "onload", "cache": true}])
    );
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 2);
}

#[test]
fn online_store_sales_channel_cold_reads_forward_and_hydrate_observed_state() {
    let theme_id = "gid://shopify/OnlineStoreTheme/701";
    let script_tag_id = "gid://shopify/ScriptTag/702";
    let web_pixel_id = "gid://shopify/WebPixel/703";
    let server_pixel_id = "gid://shopify/ServerPixel/704";
    let mobile_app_id = "gid://shopify/MobilePlatformApplication/705";
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let upstream_calls = Arc::clone(&upstream_calls);
        move |request| {
            let body: Value = serde_json::from_str(&request.body)
                .expect("upstream sales-channel read body parses");
            upstream_calls.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "theme": {
                            "__typename": "OnlineStoreTheme",
                            "id": theme_id,
                            "name": "Upstream main theme",
                            "role": "MAIN"
                        },
                        "themes": {
                            "nodes": [{
                                "__typename": "OnlineStoreTheme",
                                "id": theme_id,
                                "name": "Upstream main theme",
                                "role": "MAIN"
                            }]
                        },
                        "scriptTag": {
                            "id": script_tag_id,
                            "src": "https://cdn.example.test/upstream.js",
                            "displayScope": "ALL",
                            "event": "onload",
                            "cache": true
                        },
                        "scriptTags": {
                            "nodes": [{
                                "id": script_tag_id,
                                "src": "https://cdn.example.test/upstream.js",
                                "displayScope": "ALL",
                                "event": "onload",
                                "cache": true
                            }]
                        },
                        "webPixel": {
                            "__typename": "WebPixel",
                            "id": web_pixel_id,
                            "status": "CONNECTED",
                            "settings": {"accountID": "upstream"},
                            "webhookEndpointAddress": null
                        },
                        "serverPixel": {
                            "__typename": "ServerPixel",
                            "id": server_pixel_id,
                            "status": "CONNECTED",
                            "webhookEndpointAddress": "arn:aws:events:us-east-1:123456789012:event-bus/upstream"
                        },
                        "mobilePlatformApplication": {
                            "__typename": "AppleApplication",
                            "id": mobile_app_id,
                            "appId": "com.example.upstream",
                            "universalLinksEnabled": true
                        },
                        "mobilePlatformApplications": {
                            "nodes": [{
                                "__typename": "AppleApplication",
                                "id": mobile_app_id,
                                "appId": "com.example.upstream",
                                "universalLinksEnabled": true
                            }]
                        }
                    }
                }),
            }
        }
    });

    let read_query = r#"
        query SalesChannelColdRead($themeId: ID!, $scriptTagId: ID!, $webPixelId: ID!, $serverPixelId: ID!, $mobileAppId: ID!) {
          theme(id: $themeId) { id name role }
          themes(first: 10) { nodes { id name role } }
          scriptTag(id: $scriptTagId) { id src displayScope event cache }
          scriptTags(first: 10) { nodes { id src displayScope event cache } }
          webPixel(id: $webPixelId) { id status settings webhookEndpointAddress }
          serverPixel(id: $serverPixelId) { id status webhookEndpointAddress }
          mobilePlatformApplication(id: $mobileAppId) { __typename ... on AppleApplication { id appId universalLinksEnabled } }
          mobilePlatformApplications(first: 10) { nodes { __typename ... on AppleApplication { id appId universalLinksEnabled } } }
        }
    "#;
    let variables = json!({
        "themeId": theme_id,
        "scriptTagId": script_tag_id,
        "webPixelId": web_pixel_id,
        "serverPixelId": server_pixel_id,
        "mobileAppId": mobile_app_id
    });

    let cold_read = proxy.process_request(json_graphql_request(read_query, variables.clone()));
    assert_eq!(cold_read.status, 200);
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
    assert_eq!(
        cold_read.body["data"]["themes"]["nodes"][0]["id"],
        json!(theme_id)
    );
    assert_eq!(
        cold_read.body["data"]["scriptTags"]["nodes"][0]["id"],
        json!(script_tag_id)
    );

    let hydrated_read = proxy.process_request(json_graphql_request(read_query, variables));
    assert_eq!(
        upstream_calls.lock().unwrap().len(),
        1,
        "observed sales-channel state should satisfy the second read locally"
    );
    assert_eq!(
        hydrated_read.body["data"]["theme"],
        json!({"id": theme_id, "name": "Upstream main theme", "role": "MAIN"})
    );
    assert_eq!(
        hydrated_read.body["data"]["scriptTag"],
        json!({"id": script_tag_id, "src": "https://cdn.example.test/upstream.js", "displayScope": "ALL", "event": "onload", "cache": true})
    );
    assert_eq!(
        hydrated_read.body["data"]["webPixel"],
        json!({"id": web_pixel_id, "status": "CONNECTED", "settings": {"accountID": "upstream"}, "webhookEndpointAddress": null})
    );
    assert_eq!(
        hydrated_read.body["data"]["serverPixel"],
        json!({"id": server_pixel_id, "status": "CONNECTED", "webhookEndpointAddress": "arn:aws:events:us-east-1:123456789012:event-bus/upstream"})
    );
    assert_eq!(
        hydrated_read.body["data"]["mobilePlatformApplications"]["nodes"],
        json!([{"__typename": "AppleApplication", "id": mobile_app_id, "appId": "com.example.upstream", "universalLinksEnabled": true}])
    );
}

#[test]
fn online_store_script_tag_update_unknown_id_returns_not_found() {
    let mut proxy = snapshot_proxy();

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation ScriptTagUpdateUnknown {
          scriptTagUpdate(id: "gid://shopify/ScriptTag/999999999", input: { src: "https://cdn.example.test/changed.js" }) {
            scriptTag { id src displayScope event cache }
            userErrors {  field message }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        update.body["data"]["scriptTagUpdate"],
        json!({
            "scriptTag": null,
            "userErrors": [{
                "field": ["id"],
                "message": "Script tag not found"
            }]
        })
    );
    assert_eq!(log_snapshot(&proxy), json!({ "entries": [] }));
}

#[test]
fn online_store_storefront_access_token_edges_covers_current_behavior() {
    let mut proxy = snapshot_proxy();

    let first = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreStorefrontAccessTokenLocalRuntimeFirst {
          storefrontAccessTokenCreate(input: { title: "Hydrogen" }) {
            storefrontAccessToken { id title accessToken accessScopes { handle } }
            shop { id }
            userErrors {  field message }
          }
        }
        "#,
        json!({}),
    ));
    let first_token = first.body["data"]["storefrontAccessTokenCreate"]["storefrontAccessToken"]
        ["accessToken"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(first_token.starts_with("shpat_"));
    assert_eq!(
        first.body["data"]["storefrontAccessTokenCreate"],
        json!({
            "storefrontAccessToken": {
                "id": "gid://shopify/StorefrontAccessToken/1?shopify-draft-proxy=synthetic",
                "title": "Hydrogen",
                "accessToken": first_token,
                "accessScopes": [
                    {"handle": "unauthenticated_read_product_listings"},
                    {"handle": "unauthenticated_read_product_inventory"}
                ]
            },
            "shop": {},
            "userErrors": []
        })
    );

    let mut filtered_request = json_graphql_request(
        r#"
        mutation RustOnlineStoreStorefrontAccessTokenLocalRuntimeFilteredScopes {
          storefrontAccessTokenCreate(input: { title: "Hydrogen filtered" }) {
            storefrontAccessToken { id title accessToken accessScopes { handle } }
            userErrors {  field message }
          }
        }
        "#,
        json!({}),
    );
    filtered_request.headers.insert(
        "x-shopify-draft-proxy-access-scopes".to_string(),
        "read_products,unauthenticated_read_customers,unauthenticated_read_product_inventory,write_orders"
            .to_string(),
    );
    let filtered = proxy.process_request(filtered_request);
    let filtered_token = filtered.body["data"]["storefrontAccessTokenCreate"]
        ["storefrontAccessToken"]["accessToken"]
        .as_str()
        .unwrap();
    assert!(filtered_token.starts_with("shpat_"));
    assert_ne!(filtered_token, first_token);
    let first_token_value = u64::from_str_radix(first_token.trim_start_matches("shpat_"), 16)
        .expect("first synthetic StorefrontAccessToken should be hex encoded");
    let filtered_token_value = u64::from_str_radix(filtered_token.trim_start_matches("shpat_"), 16)
        .expect("second synthetic StorefrontAccessToken should be hex encoded");
    assert_eq!(
        filtered_token_value.wrapping_sub(first_token_value),
        1,
        "synthetic StorefrontAccessToken bytes should advance uniformly with the id"
    );
    assert_eq!(
        filtered.body["data"]["storefrontAccessTokenCreate"]["storefrontAccessToken"]
            ["accessScopes"],
        json!([
            {"handle": "unauthenticated_read_customers"},
            {"handle": "unauthenticated_read_product_inventory"}
        ])
    );

    let third = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreStorefrontAccessTokenLocalRuntimeThird {
          storefrontAccessTokenCreate(input: { title: "Hydrogen third" }) {
            storefrontAccessToken { accessToken }
            userErrors {  field message }
          }
        }
        "#,
        json!({}),
    ));
    let third_token = third.body["data"]["storefrontAccessTokenCreate"]["storefrontAccessToken"]
        ["accessToken"]
        .as_str()
        .unwrap();
    let third_token_value = u64::from_str_radix(third_token.trim_start_matches("shpat_"), 16)
        .expect("third synthetic StorefrontAccessToken should be hex encoded");
    assert_eq!(
        third_token_value.wrapping_sub(filtered_token_value),
        1,
        "synthetic StorefrontAccessToken bytes should not special-case id suffixes"
    );

    let blank = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreStorefrontAccessTokenLocalRuntimeBlankTitle {
          storefrontAccessTokenCreate(input: { title: "   " }) {
            storefrontAccessToken { id }
            shop { id }
            userErrors {  field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        blank.body["data"]["storefrontAccessTokenCreate"],
        json!({
            "storefrontAccessToken": null,
            "shop": {},
            "userErrors": [{"field": ["input", "title"], "message": "Title can't be blank"}]
        })
    );

    for index in 0..97 {
        let fill = proxy.process_request(json_graphql_request(
            r#"
            mutation RustOnlineStoreStorefrontAccessTokenLocalRuntimeFill($title: String!) {
              storefrontAccessTokenCreate(input: { title: $title }) {
                storefrontAccessToken { id }
                userErrors {  field message }
              }
            }
            "#,
            json!({"title": format!("token {index}")}),
        ));
        assert_eq!(
            fill.body["data"]["storefrontAccessTokenCreate"]["userErrors"],
            json!([])
        );
    }

    let limit = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreStorefrontAccessTokenLocalRuntimeLimit {
          storefrontAccessTokenCreate(input: { title: "One too many" }) {
            storefrontAccessToken { id }
            userErrors {  field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        limit.body["data"]["storefrontAccessTokenCreate"],
        json!({
            "storefrontAccessToken": null,
            "userErrors": [{"field": ["input"], "message": "apps.admin.graph_api_errors.storefront_access_token_create.reached_limit"}]
        })
    );
}

#[test]
fn web_pixel_create_success_returns_connected_with_non_null_settings() {
    let mut omitted_proxy = snapshot_proxy();
    let omitted = omitted_proxy.process_request(json_graphql_request(
        r#"
        mutation WebPixelUpdateValidationLocalRuntimeOmittedSettings {
          webPixelCreate(webPixel: {}) {
            webPixel { id status settings }
            userErrors { __typename code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        omitted.body["data"]["webPixelCreate"],
        json!({
            "webPixel": {
                "id": "gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic",
                "status": "CONNECTED",
                "settings": {}
            },
            "userErrors": []
        })
    );

    let mut empty_json_proxy = snapshot_proxy();
    let empty_json = empty_json_proxy.process_request(json_graphql_request(
        r#"
        mutation WebPixelUpdateValidationLocalRuntimeEmptyJsonSettings {
          webPixelCreate(webPixel: { settings: "{}" }) {
            webPixel { id status settings }
            userErrors { __typename code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        empty_json.body["data"]["webPixelCreate"],
        json!({
            "webPixel": {
                "id": "gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic",
                "status": "CONNECTED",
                "settings": {}
            },
            "userErrors": []
        })
    );

    let mut object_proxy = snapshot_proxy();
    let object = object_proxy.process_request(json_graphql_request(
        r#"
        mutation WebPixelUpdateValidationLocalRuntimeObjectSettings {
          webPixelCreate(webPixel: { settings: { accountID: "abc" } }) {
            webPixel { id status settings }
            userErrors { __typename code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        object.body["data"]["webPixelCreate"],
        json!({
            "webPixel": {
                "id": "gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic",
                "status": "CONNECTED",
                "settings": {"accountID": "abc"}
            },
            "userErrors": []
        })
    );
}

#[test]
fn online_store_pixel_endpoint_edges_covers_current_behavior() {
    let mut proxy = snapshot_proxy();

    let web_pixel = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStorePixelLocalRuntimeEdges {
          create: webPixelCreate(webPixel: {}) { webPixel { id status settings webhookEndpointAddress } userErrors { __typename code field message } }
          duplicate: webPixelCreate(webPixel: { settings: "{\"accountID\":\"abc\"}" }) { webPixel { id status } userErrors { __typename code field message } }
          missingUpdate: webPixelUpdate(id: "gid://shopify/WebPixel/9999999999", webPixel: { settings: "{}" }) { webPixel { id } userErrors { __typename code field message } }
          invalidJson: webPixelUpdate(id: "gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic", webPixel: { settings: "not json" }) { webPixel { id settings status } userErrors { __typename code field message } }
          validUpdate: webPixelUpdate(id: "gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic", webPixel: { settings: "{\"accountID\":\"abc\"}" }) { webPixel { id settings status webhookEndpointAddress } userErrors { __typename code field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        web_pixel.body["data"]["create"],
        json!({"webPixel": {"id": "gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic", "status": "CONNECTED", "settings": {}, "webhookEndpointAddress": null}, "userErrors": []})
    );
    assert_eq!(
        web_pixel.body["data"]["duplicate"],
        json!({"webPixel": null, "userErrors": [{"__typename": "WebPixelUserError", "code": "TAKEN", "field": null, "message": "Web pixel is taken."}]})
    );
    assert_eq!(
        web_pixel.body["data"]["missingUpdate"]["userErrors"][0]["code"],
        json!("NOT_FOUND")
    );
    assert_eq!(
        web_pixel.body["data"]["invalidJson"]["userErrors"][0]["code"],
        json!("INVALID_CONFIGURATION_JSON")
    );
    assert_eq!(
        web_pixel.body["data"]["validUpdate"]["webPixel"],
        json!({"id": "gid://shopify/WebPixel/1?shopify-draft-proxy=synthetic", "settings": {"accountID": "abc"}, "status": "CONNECTED", "webhookEndpointAddress": null})
    );

    let missing_server = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreServerPixelMissingEndpointUpdate {
          eventBridgeServerPixelUpdate(arn: "arn:aws:events:us-east-1:123456789012:event-bus/missing") {
            serverPixel { id webhookEndpointAddress }
            userErrors { __typename code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        missing_server.body["data"]["eventBridgeServerPixelUpdate"],
        json!({"serverPixel": null, "userErrors": [{"__typename": "ServerPixelUserError", "code": "NOT_FOUND", "field": ["id"], "message": "Server pixel not found"}]})
    );

    // Valid endpoint updates execute and stage normally. Each erroring case
    // below is issued on its own because server-pixel argument validation raises
    // a top-level GraphQL error (no `data`) before any field executes — bundling
    // them would mask all but the first error.
    let server_pixel = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreServerPixelEndpointLocalRuntimeEdges {
          create: serverPixelCreate { serverPixel { id status webhookEndpointAddress } userErrors { __typename code field message } }
          eventBridge: eventBridgeServerPixelUpdate(arn: "arn:aws:events:us-east-1:123456789012:event-bus/local") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } }
          pubsub: pubSubServerPixelUpdate(pubSubProject: "project", pubSubTopic: "topic") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        server_pixel.body["data"]["create"],
        json!({"serverPixel": {"id": "gid://shopify/ServerPixel/2?shopify-draft-proxy=synthetic", "status": "CONNECTED", "webhookEndpointAddress": null}, "userErrors": []})
    );
    assert_eq!(
        server_pixel.body["data"]["eventBridge"]["serverPixel"]["webhookEndpointAddress"],
        json!("arn:aws:events:us-east-1:123456789012:event-bus/local")
    );
    assert_eq!(
        server_pixel.body["data"]["pubsub"]["serverPixel"]["webhookEndpointAddress"],
        json!("project/topic")
    );

    // A malformed ARN fails ARN-scalar coercion: a top-level CoercionError with
    // no data. This local-only branch is covered here rather than by parity evidence.
    let invalid_arn = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreServerPixelInvalidArn {
          eventBridgeServerPixelUpdate(arn: "not-an-arn") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(invalid_arn.body["data"], Value::Null);
    assert_eq!(
        invalid_arn.body["errors"],
        json!([{"message": "Invalid ARN 'not-an-arn'", "extensions": {"code": "argumentLiteralsIncompatible", "typeName": "CoercionError"}}])
    );

    // A blank Pub/Sub project is an INVALID_FIELD_ARGUMENTS top-level error.
    // Shopify surfaces the first blank required field, not a per-field userError array.
    let blank_pub_sub = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreServerPixelBlankPubSub {
          pubSubServerPixelUpdate(pubSubProject: "", pubSubTopic: " ") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(blank_pub_sub.body["data"], Value::Null);
    assert_eq!(
        blank_pub_sub.body["errors"],
        json!([{"message": "pubSubProject can't be blank", "extensions": {"code": "INVALID_FIELD_ARGUMENTS"}, "path": ["pubSubServerPixelUpdate"]}])
    );
}

#[test]
fn webhook_eventbridge_arn_validation_uses_shopify_partner_shape_and_fields() {
    let mut proxy = snapshot_proxy();
    let create_mutation = r#"
        mutation RustWebhookLocalRuntimeEventBridgeValidation($webhookSubscription: EventBridgeWebhookSubscriptionInput!) {
          eventBridgeWebhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id endpoint { __typename ... on WebhookEventBridgeEndpoint { arn } } }
            userErrors { field message }
          }
        }
    "#;
    let mut missing_source_request = json_graphql_request(
        create_mutation,
        json!({"webhookSubscription": {"arn": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713"}}),
    );
    missing_source_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let missing_source = proxy.process_request(missing_source_request);
    assert_eq!(
        missing_source.body["data"]["eventBridgeWebhookSubscriptionCreate"],
        json!({"webhookSubscription": null, "userErrors": [
            {"field": ["webhookSubscription", "arn"], "message": "Address is invalid"},
            {"field": ["webhookSubscription", "arn"], "message": "Address is not a valid AWS ARN"}
        ]})
    );

    let mut wrong_client_request = json_graphql_request(
        create_mutation,
        json!({"webhookSubscription": {"arn": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/1/source-x"}}),
    );
    wrong_client_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let wrong_client = proxy.process_request(wrong_client_request);
    assert_eq!(
        wrong_client.body["data"]["eventBridgeWebhookSubscriptionCreate"],
        json!({"webhookSubscription": null, "userErrors": [
            {"field": ["webhookSubscription", "arn"], "message": "Address is invalid"},
            {"field": ["webhookSubscription", "arn"], "message": "Address is an AWS ARN and includes api_client_id '1' instead of '347082227713'"}
        ]})
    );

    let mut generic_arn_request = json_graphql_request(
        create_mutation,
        json!({"webhookSubscription": {"arn": "arn:aws:events:us-east-1:123456789012:rule/foo"}}),
    );
    generic_arn_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let generic_arn = proxy.process_request(generic_arn_request);
    assert_eq!(
        generic_arn.body["data"]["eventBridgeWebhookSubscriptionCreate"],
        json!({"webhookSubscription": null, "userErrors": [
            {"field": ["webhookSubscription", "arn"], "message": "Address is invalid"},
            {"field": ["webhookSubscription", "arn"], "message": "Address is not a valid AWS ARN"}
        ]})
    );

    let mut accepted_request = json_graphql_request(
        create_mutation,
        json!({"webhookSubscription": {"arn": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/347082227713/source-x"}}),
    );
    accepted_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let accepted = proxy.process_request(accepted_request);
    let subscription_id = accepted.body["data"]["eventBridgeWebhookSubscriptionCreate"]
        ["webhookSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        accepted.body["data"]["eventBridgeWebhookSubscriptionCreate"]["userErrors"],
        json!([])
    );

    let update_mutation = r#"
        mutation RustWebhookLocalRuntimeEventBridgeUpdateValidation($id: ID!, $webhookSubscription: EventBridgeWebhookSubscriptionInput!) {
          eventBridgeWebhookSubscriptionUpdate(id: $id, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id }
            userErrors { field message }
          }
        }
    "#;
    let mut update_request = json_graphql_request(
        update_mutation,
        json!({
            "id": subscription_id,
            "webhookSubscription": {"arn": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/1/source-x"}
        }),
    );
    update_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let update = proxy.process_request(update_request);
    assert_eq!(
        update.body["data"]["eventBridgeWebhookSubscriptionUpdate"],
        json!({"webhookSubscription": null, "userErrors": [
            {"field": ["webhookSubscription", "arn"], "message": "Address is invalid"},
            {"field": ["webhookSubscription", "arn"], "message": "Address is an AWS ARN and includes api_client_id '1' instead of '347082227713'"}
        ]})
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        ..Default::default()
    });
    assert_eq!(log.body["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn webhook_cloud_destination_validation_preserves_unified_and_pubsub_fields() {
    let mut proxy = snapshot_proxy();
    let unified_mutation = r#"
        mutation RustWebhookLocalRuntimeUnifiedCloudValidation($webhookSubscription: WebhookSubscriptionInput!) {
          webhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id }
            userErrors { field message }
          }
        }
    "#;
    let mut unified_request = json_graphql_request(
        unified_mutation,
        json!({"webhookSubscription": {"callbackUrl": "arn:aws:events:us-east-1::event-source/aws.partner/shopify.com/1/source-x"}}),
    );
    unified_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "347082227713".to_string(),
    );
    let unified = proxy.process_request(unified_request);
    assert_eq!(
        unified.body["data"]["webhookSubscriptionCreate"],
        json!({"webhookSubscription": null, "userErrors": [
            {"field": ["webhookSubscription", "callbackUrl"], "message": "Address is invalid"},
            {"field": ["webhookSubscription", "callbackUrl"], "message": "Address is an AWS ARN and includes api_client_id '1' instead of '347082227713'"}
        ]})
    );

    let pubsub_create = r#"
        mutation RustWebhookLocalRuntimePubSubProjectValidation($webhookSubscription: PubSubWebhookSubscriptionInput!) {
          pubSubWebhookSubscriptionCreate(topic: SHOP_UPDATE, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id }
            userErrors { field message }
          }
        }
    "#;
    let bad_project_create = proxy.process_request(json_graphql_request(
        pubsub_create,
        json!({"webhookSubscription": {"pubSubProject": "-bad-project", "pubSubTopic": "valid-topic"}}),
    ));
    assert_eq!(
        bad_project_create.body["data"]["pubSubWebhookSubscriptionCreate"],
        json!({"webhookSubscription": null, "userErrors": [{
            "field": ["webhookSubscription", "pubSubProject"],
            "message": "Google Cloud Pub/Sub project ID is not valid"
        }]})
    );

    let valid_project_create = proxy.process_request(json_graphql_request(
        pubsub_create,
        json!({"webhookSubscription": {"pubSubProject": "valid-project", "pubSubTopic": "valid-topic"}}),
    ));
    let subscription_id = valid_project_create.body["data"]["pubSubWebhookSubscriptionCreate"]
        ["webhookSubscription"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        valid_project_create.body["data"]["pubSubWebhookSubscriptionCreate"]["userErrors"],
        json!([])
    );

    let pubsub_update = r#"
        mutation RustWebhookLocalRuntimePubSubProjectUpdateValidation($id: ID!, $webhookSubscription: PubSubWebhookSubscriptionInput!) {
          pubSubWebhookSubscriptionUpdate(id: $id, webhookSubscription: $webhookSubscription) {
            webhookSubscription { id }
            userErrors { field message }
          }
        }
    "#;
    let bad_project_update = proxy.process_request(json_graphql_request(
        pubsub_update,
        json!({
            "id": subscription_id,
            "webhookSubscription": {"pubSubProject": "-bad-project", "pubSubTopic": "valid-topic"}
        }),
    ));
    assert_eq!(
        bad_project_update.body["data"]["pubSubWebhookSubscriptionUpdate"],
        json!({"webhookSubscription": null, "userErrors": [{
            "field": ["webhookSubscription", "pubSubProject"],
            "message": "Google Cloud Pub/Sub project ID is not valid"
        }]})
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        ..Default::default()
    });
    assert_eq!(log.body["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn online_store_theme_lifecycle_tail_helpers_cover_current_behavior() {
    let mut proxy = snapshot_proxy();

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeLocalRuntimeCreate {
          first: themeCreate(source: "https://example.com/current.zip", name: "Current main", role: MAIN) { theme { id role name } userErrors { field message code } }
          second: themeCreate(source: "https://example.com/next.zip", name: "Next main", role: UNPUBLISHED) { theme { id role name } userErrors { field message code } }
          demo: themeCreate(source: "https://example.com/demo.zip", name: "Demo theme", role: DEMO) { theme { id role name } userErrors { field message code } }
          locked: themeCreate(source: "https://example.com/locked.zip", name: "Locked fixture", role: LOCKED) { theme { id role name } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        created.body["data"]["first"]["theme"],
        json!({"id": "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", "role": "MAIN", "name": "Current main"})
    );
    assert_eq!(
        created.body["data"]["second"]["theme"]["role"],
        json!("UNPUBLISHED")
    );

    let publish = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeLocalRuntimePublish {
          publishSecond: themePublish(id: "gid://shopify/OnlineStoreTheme/2?shopify-draft-proxy=synthetic") { theme { id role } userErrors { field message } }
          rejectDemo: themePublish(id: "gid://shopify/OnlineStoreTheme/3?shopify-draft-proxy=synthetic") { theme { id role } userErrors { field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        publish.body["data"]["publishSecond"],
        json!({"theme": {"id": "gid://shopify/OnlineStoreTheme/2?shopify-draft-proxy=synthetic", "role": "MAIN"}, "userErrors": []})
    );
    assert_eq!(
        publish.body["data"]["rejectDemo"],
        json!({"theme": null, "userErrors": [{"field": ["id"], "message": "Theme cannot be published from role DEMO"}]})
    );

    let read_after_publish = proxy.process_request(json_graphql_request(
        r#"
        query RustOnlineStoreThemeLocalRuntimeReadAfterPublish {
          previous: theme(id: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic") { id role name }
          mains: themes(first: 10, roles: [MAIN]) { nodes { id role name } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read_after_publish.body["data"]["previous"],
        json!({"id": "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", "role": "UNPUBLISHED", "name": "Current main"})
    );
    assert_eq!(
        read_after_publish.body["data"]["mains"]["nodes"],
        json!([{"id": "gid://shopify/OnlineStoreTheme/2?shopify-draft-proxy=synthetic", "role": "MAIN", "name": "Next main"}])
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeLocalRuntimeUpdate {
          locked: themeUpdate(id: "gid://shopify/OnlineStoreTheme/4?shopify-draft-proxy=synthetic", input: { name: "Renamed" }) { theme { id role name } userErrors { field message code } }
          blank: themeUpdate(id: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", input: { name: "   " }) { theme { id role name } userErrors { field message code } }
          valid: themeUpdate(id: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", input: { name: "Renamed fixture" }) { theme { id role name } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        update.body["data"]["locked"],
        json!({"theme": null, "userErrors": [{"field": ["id"], "message": "Locked themes cannot be modified.", "code": "CANNOT_UPDATE_LOCKED_THEME"}]})
    );
    assert_eq!(
        update.body["data"]["blank"],
        json!({"theme": null, "userErrors": [{"field": ["input", "name"], "message": "Name can't be blank", "code": "INVALID"}]})
    );
    assert_eq!(
        update.body["data"]["valid"],
        json!({"theme": {"id": "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", "role": "UNPUBLISHED", "name": "Renamed fixture"}, "userErrors": []})
    );

    let delete_only_main_proxy = {
        let mut proxy = snapshot_proxy();
        proxy.process_request(json_graphql_request(
            r#"
            mutation RustOnlineStoreThemeLocalRuntimeOnlyMainSetup {
              themeCreate(source: "https://example.com/current.zip", name: "Only main", role: MAIN) { theme { id role name } userErrors { field message code } }
            }
            "#,
            json!({}),
        ));
        proxy
    };
    let mut delete_only_main_proxy = delete_only_main_proxy;
    let only_main = delete_only_main_proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeLocalRuntimeOnlyMainDelete {
          themeDelete(id: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic") { deletedThemeId userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        only_main.body["data"]["themeDelete"],
        json!({"deletedThemeId": null, "userErrors": [{"field": ["id"], "message": "You can't delete your only published theme.", "code": "INVALID"}]})
    );

    let delete_non_main = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeLocalRuntimeDeleteNonMain {
          deleteDemo: themeDelete(id: "gid://shopify/OnlineStoreTheme/3?shopify-draft-proxy=synthetic") { deletedThemeId userErrors { field message code } }
          deleteFormerMain: themeDelete(id: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic") { deletedThemeId userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        delete_non_main.body["data"]["deleteDemo"],
        json!({"deletedThemeId": "gid://shopify/OnlineStoreTheme/3?shopify-draft-proxy=synthetic", "userErrors": []})
    );
    assert_eq!(
        delete_non_main.body["data"]["deleteFormerMain"],
        json!({"deletedThemeId": "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", "userErrors": []})
    );
}

#[test]
fn online_store_theme_publish_rejects_development_without_role_changes() {
    let mut proxy = snapshot_proxy();

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeLocalRuntimeDevelopmentPublishSetup {
          main: themeCreate(source: "https://example.com/current.zip", name: "Current main", role: MAIN) { theme { id role name } userErrors { field message code } }
          development: themeCreate(source: "https://example.com/dev.zip", name: "Development theme", role: DEVELOPMENT) { theme { id role name } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        created.body["data"]["main"]["theme"],
        json!({"id": "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", "role": "MAIN", "name": "Current main"})
    );
    assert_eq!(
        created.body["data"]["development"]["theme"],
        json!({"id": "gid://shopify/OnlineStoreTheme/2?shopify-draft-proxy=synthetic", "role": "DEVELOPMENT", "name": "Development theme"})
    );

    let publish = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeLocalRuntimeDevelopmentPublish {
          themePublish(id: "gid://shopify/OnlineStoreTheme/2?shopify-draft-proxy=synthetic") {
            theme { id role }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        publish.body["data"]["themePublish"],
        json!({"theme": null, "userErrors": [{"field": ["base"], "message": "You cannot publish a development theme.", "code": null}]})
    );

    let read_after_publish = proxy.process_request(json_graphql_request(
        r#"
        query RustOnlineStoreThemeLocalRuntimeDevelopmentPublishRead {
          main: theme(id: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic") { id role name }
          development: theme(id: "gid://shopify/OnlineStoreTheme/2?shopify-draft-proxy=synthetic") { id role name }
          mains: themes(first: 10, roles: [MAIN]) { nodes { id role name } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read_after_publish.body["data"]["main"],
        json!({"id": "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", "role": "MAIN", "name": "Current main"})
    );
    assert_eq!(
        read_after_publish.body["data"]["development"],
        json!({"id": "gid://shopify/OnlineStoreTheme/2?shopify-draft-proxy=synthetic", "role": "DEVELOPMENT", "name": "Development theme"})
    );
    assert_eq!(
        read_after_publish.body["data"]["mains"]["nodes"],
        json!([{"id": "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", "role": "MAIN", "name": "Current main"}])
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        ..Default::default()
    });
    assert_eq!(log.body["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn online_store_theme_connection_paginates_edges_nodes_and_page_info_consistently() {
    let mut proxy = snapshot_proxy();

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeLocalRuntimeCreate {
          first: themeCreate(source: "https://example.com/first.zip", name: "First theme", role: UNPUBLISHED) { theme { id } userErrors { field message code } }
          second: themeCreate(source: "https://example.com/second.zip", name: "Second theme", role: UNPUBLISHED) { theme { id } userErrors { field message code } }
          third: themeCreate(source: "https://example.com/third.zip", name: "Third theme", role: UNPUBLISHED) { theme { id } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(created.body["data"]["first"]["userErrors"], json!([]));
    assert_eq!(created.body["data"]["second"]["userErrors"], json!([]));
    assert_eq!(created.body["data"]["third"]["userErrors"], json!([]));

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query RustOnlineStoreThemeLocalRuntimeReadAfterPublish($first: Int!) {
          themes(first: $first) {
            nodes { id name }
            edges { cursor node { id name } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"first": 2}),
    ));
    assert_eq!(
        first_page.body["data"]["themes"],
        json!({
            "nodes": [
                {"id": "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", "name": "First theme"},
                {"id": "gid://shopify/OnlineStoreTheme/2?shopify-draft-proxy=synthetic", "name": "Second theme"}
            ],
            "edges": [
                {"cursor": "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", "node": {"id": "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", "name": "First theme"}},
                {"cursor": "gid://shopify/OnlineStoreTheme/2?shopify-draft-proxy=synthetic", "node": {"id": "gid://shopify/OnlineStoreTheme/2?shopify-draft-proxy=synthetic", "name": "Second theme"}}
            ],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic",
                "endCursor": "gid://shopify/OnlineStoreTheme/2?shopify-draft-proxy=synthetic"
            }
        })
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query RustOnlineStoreThemeLocalRuntimeReadAfterPublish($first: Int!, $after: String!) {
          themes(first: $first, after: $after) {
            nodes { id name }
            edges { cursor node { id name } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"first": 2, "after": first_page.body["data"]["themes"]["pageInfo"]["endCursor"]}),
    ));
    assert_eq!(
        second_page.body["data"]["themes"],
        json!({
            "nodes": [{"id": "gid://shopify/OnlineStoreTheme/3?shopify-draft-proxy=synthetic", "name": "Third theme"}],
            "edges": [{"cursor": "gid://shopify/OnlineStoreTheme/3?shopify-draft-proxy=synthetic", "node": {"id": "gid://shopify/OnlineStoreTheme/3?shopify-draft-proxy=synthetic", "name": "Third theme"}}],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": "gid://shopify/OnlineStoreTheme/3?shopify-draft-proxy=synthetic",
                "endCursor": "gid://shopify/OnlineStoreTheme/3?shopify-draft-proxy=synthetic"
            }
        })
    );
}

#[test]
fn online_store_theme_file_lifecycle_tail_helpers_cover_current_behavior() {
    let mut proxy = snapshot_proxy();

    proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeFileLocalRuntimeCreate {
          themeCreate(source: "https://example.com/theme.zip", name: "HAR 585 Theme") { theme { id } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));

    let upserts = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeFileLocalRuntimeUpsert {
          first: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "templates/index.json", body: { type: TEXT, value: "hello" } }]) { upsertedThemeFiles { filename createdAt updatedAt checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } }
          second: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "templates/index.json", body: { type: TEXT, value: "hello world" } }]) { upsertedThemeFiles { filename createdAt updatedAt checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } }
          invalid: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "evil/path.liquid", body: { type: TEXT, value: "ignored" } }]) { upsertedThemeFiles { filename } userErrors { field message code } }
          app: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "assets/app.js", body: { type: TEXT, value: "console.log(1)" } }]) { upsertedThemeFiles { filename createdAt updatedAt } userErrors { field message code } }
          theme: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "assets/theme.js", body: { type: TEXT, value: "hello" } }]) { upsertedThemeFiles { filename createdAt updatedAt } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    let first_file = &upserts.body["data"]["first"]["upsertedThemeFiles"][0];
    let first_created_at = assert_online_store_operation_timestamp(
        &first_file["createdAt"],
        "themeFilesUpsert.first.createdAt",
    );
    let first_updated_at = assert_online_store_operation_timestamp(
        &first_file["updatedAt"],
        "themeFilesUpsert.first.updatedAt",
    );
    assert_eq!(first_created_at, first_updated_at);
    assert_eq!(
        first_file,
        &json!({"filename": "templates/index.json", "createdAt": first_created_at.clone(), "updatedAt": first_updated_at.clone(), "checksumMd5": "5d41402abc4b2a76b9719d911017c592", "size": 5, "body": {"content": "hello"}})
    );
    let second_file = &upserts.body["data"]["second"]["upsertedThemeFiles"][0];
    let second_created_at = assert_online_store_operation_timestamp(
        &second_file["createdAt"],
        "themeFilesUpsert.second.createdAt",
    );
    let second_updated_at = assert_online_store_operation_timestamp(
        &second_file["updatedAt"],
        "themeFilesUpsert.second.updatedAt",
    );
    assert_eq!(second_created_at, first_created_at);
    assert_eq!(
        second_file,
        &json!({"filename": "templates/index.json", "createdAt": second_created_at.clone(), "updatedAt": second_updated_at.clone(), "checksumMd5": "5eb63bbbe01eeed093cb22bb8f5acdc3", "size": 11, "body": {"content": "hello world"}})
    );
    let app_file = &upserts.body["data"]["app"]["upsertedThemeFiles"][0];
    let app_created_at = assert_online_store_operation_timestamp(
        &app_file["createdAt"],
        "themeFilesUpsert.app.createdAt",
    );
    let app_updated_at = assert_online_store_operation_timestamp(
        &app_file["updatedAt"],
        "themeFilesUpsert.app.updatedAt",
    );
    assert_eq!(app_file["filename"], json!("assets/app.js"));
    assert_eq!(app_created_at, app_updated_at);
    let theme_file = &upserts.body["data"]["theme"]["upsertedThemeFiles"][0];
    let theme_created_at = assert_online_store_operation_timestamp(
        &theme_file["createdAt"],
        "themeFilesUpsert.theme.createdAt",
    );
    let theme_updated_at = assert_online_store_operation_timestamp(
        &theme_file["updatedAt"],
        "themeFilesUpsert.theme.updatedAt",
    );
    assert_eq!(theme_file["filename"], json!("assets/theme.js"));
    assert_eq!(theme_created_at, theme_updated_at);
    assert_eq!(
        upserts.body["data"]["invalid"],
        json!({"upsertedThemeFiles": [], "userErrors": [{"field": ["files", "0", "filename"], "message": "Filename is invalid", "code": "INVALID"}]})
    );

    let copy_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeFileLocalRuntimeCopyDelete {
          missingCopy: themeFilesCopy(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ srcFilename: "assets/missing.js", dstFilename: "assets/copy.js" }]) { copiedThemeFiles { filename } userErrors { field message code } }
          copy: themeFilesCopy(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ srcFilename: "assets/app.js", dstFilename: "assets/copy.js" }]) { copiedThemeFiles { filename createdAt updatedAt checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } }
          multiCopy: themeFilesCopy(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ srcFilename: "assets/app.js", dstFilename: "assets/app-copy.js" }, { srcFilename: "assets/theme.js", dstFilename: "assets/theme-copy.js" }]) { copiedThemeFiles { filename createdAt updatedAt checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } }
          mixedCopy: themeFilesCopy(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ srcFilename: "assets/missing.js", dstFilename: "assets/missing-copy.js" }, { srcFilename: "assets/theme.js", dstFilename: "assets/theme-copy-2.js" }]) { copiedThemeFiles { filename createdAt updatedAt } userErrors { field message code } }
          requiredDelete: themeFilesDelete(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: ["config/settings_data.json", "config/settings_schema.json"]) { deletedThemeFiles { filename } userErrors { field message code } }
          deleteCopy: themeFilesDelete(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: ["assets/copy.js"]) { deletedThemeFiles { filename createdAt updatedAt checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        copy_delete.body["data"]["missingCopy"],
        json!({"copiedThemeFiles": [], "userErrors": [{"field": ["files", "0", "srcFilename"], "message": "File not found", "code": "NOT_FOUND"}]})
    );
    let copied_file = &copy_delete.body["data"]["copy"]["copiedThemeFiles"][0];
    let copy_created_at = assert_online_store_operation_timestamp(
        &copied_file["createdAt"],
        "themeFilesCopy.copy.createdAt",
    );
    let copy_updated_at = assert_online_store_operation_timestamp(
        &copied_file["updatedAt"],
        "themeFilesCopy.copy.updatedAt",
    );
    assert_eq!(copy_created_at, copy_updated_at);
    assert_eq!(
        copied_file,
        &json!({"filename": "assets/copy.js", "createdAt": copy_created_at.clone(), "updatedAt": copy_updated_at.clone(), "checksumMd5": "6114f5adc373accd7b2051bd87078f62", "size": 14, "body": {"content": "console.log(1)"}})
    );
    let app_copy_file = &copy_delete.body["data"]["multiCopy"]["copiedThemeFiles"][0];
    let app_copy_created_at = assert_online_store_operation_timestamp(
        &app_copy_file["createdAt"],
        "themeFilesCopy.app-copy.createdAt",
    );
    let app_copy_updated_at = assert_online_store_operation_timestamp(
        &app_copy_file["updatedAt"],
        "themeFilesCopy.app-copy.updatedAt",
    );
    assert_eq!(app_copy_created_at, app_copy_updated_at);
    let theme_copy_file = &copy_delete.body["data"]["multiCopy"]["copiedThemeFiles"][1];
    let theme_copy_created_at = assert_online_store_operation_timestamp(
        &theme_copy_file["createdAt"],
        "themeFilesCopy.theme-copy.createdAt",
    );
    let theme_copy_updated_at = assert_online_store_operation_timestamp(
        &theme_copy_file["updatedAt"],
        "themeFilesCopy.theme-copy.updatedAt",
    );
    assert_eq!(theme_copy_created_at, theme_copy_updated_at);
    assert_eq!(
        copy_delete.body["data"]["multiCopy"],
        json!({"copiedThemeFiles": [
            {"filename": "assets/app-copy.js", "createdAt": app_copy_created_at.clone(), "updatedAt": app_copy_updated_at.clone(), "checksumMd5": "6114f5adc373accd7b2051bd87078f62", "size": 14, "body": {"content": "console.log(1)"}},
            {"filename": "assets/theme-copy.js", "createdAt": theme_copy_created_at.clone(), "updatedAt": theme_copy_updated_at.clone(), "checksumMd5": "5d41402abc4b2a76b9719d911017c592", "size": 5, "body": {"content": "hello"}}
        ], "userErrors": []})
    );
    let theme_copy_2_file = &copy_delete.body["data"]["mixedCopy"]["copiedThemeFiles"][0];
    let theme_copy_2_created_at = assert_online_store_operation_timestamp(
        &theme_copy_2_file["createdAt"],
        "themeFilesCopy.theme-copy-2.createdAt",
    );
    let theme_copy_2_updated_at = assert_online_store_operation_timestamp(
        &theme_copy_2_file["updatedAt"],
        "themeFilesCopy.theme-copy-2.updatedAt",
    );
    assert_eq!(theme_copy_2_created_at, theme_copy_2_updated_at);
    assert_eq!(
        copy_delete.body["data"]["mixedCopy"],
        json!({"copiedThemeFiles": [{"filename": "assets/theme-copy-2.js", "createdAt": theme_copy_2_created_at.clone(), "updatedAt": theme_copy_2_updated_at.clone()}], "userErrors": [{"field": ["files", "0", "srcFilename"], "message": "File not found", "code": "NOT_FOUND"}]})
    );
    assert_eq!(
        copy_delete.body["data"]["requiredDelete"]["userErrors"],
        json!([
            {"field": ["files", "0"], "message": "File is required and can't be deleted", "code": "INVALID"},
            {"field": ["files", "1"], "message": "File is required and can't be deleted", "code": "INVALID"}
        ])
    );
    assert_eq!(
        copy_delete.body["data"]["deleteCopy"],
        json!({"deletedThemeFiles": [{"filename": "assets/copy.js", "createdAt": copy_created_at.clone(), "updatedAt": copy_updated_at.clone(), "checksumMd5": "6114f5adc373accd7b2051bd87078f62", "size": 14, "body": {"content": "console.log(1)"}}], "userErrors": []})
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query RustOnlineStoreThemeFileLocalRuntimeRead {
          theme(id: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic") { files(first: 10) { nodes { filename createdAt updatedAt checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["theme"]["files"]["nodes"],
        json!([
            {"filename": "templates/index.json", "createdAt": second_created_at, "updatedAt": second_updated_at, "checksumMd5": "5eb63bbbe01eeed093cb22bb8f5acdc3", "size": 11, "body": {"content": "hello world"}},
            {"filename": "assets/app.js", "createdAt": app_created_at, "updatedAt": app_updated_at, "checksumMd5": "6114f5adc373accd7b2051bd87078f62", "size": 14, "body": {"content": "console.log(1)"}},
            {"filename": "assets/theme.js", "createdAt": theme_created_at, "updatedAt": theme_updated_at, "checksumMd5": "5d41402abc4b2a76b9719d911017c592", "size": 5, "body": {"content": "hello"}},
            {"filename": "assets/app-copy.js", "createdAt": app_copy_created_at, "updatedAt": app_copy_updated_at, "checksumMd5": "6114f5adc373accd7b2051bd87078f62", "size": 14, "body": {"content": "console.log(1)"}},
            {"filename": "assets/theme-copy.js", "createdAt": theme_copy_created_at, "updatedAt": theme_copy_updated_at, "checksumMd5": "5d41402abc4b2a76b9719d911017c592", "size": 5, "body": {"content": "hello"}},
            {"filename": "assets/theme-copy-2.js", "createdAt": theme_copy_2_created_at, "updatedAt": theme_copy_2_updated_at, "checksumMd5": "5d41402abc4b2a76b9719d911017c592", "size": 5, "body": {"content": "hello"}}
        ])
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        ..Default::default()
    });
    assert_eq!(log.body["entries"].as_array().unwrap().len(), 3);
    assert!(log.body["entries"][1]["rawBody"]
        .as_str()
        .unwrap()
        .contains("RustOnlineStoreThemeFileLocalRuntimeUpsert"));
    assert!(log.body["entries"][2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("RustOnlineStoreThemeFileLocalRuntimeCopyDelete"));
}

#[test]
fn online_store_theme_files_upsert_computes_body_modes_and_checksum_conflicts() {
    let mut proxy = snapshot_proxy();

    proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeFileValidationCreate {
          themeCreate(source: "https://example.com/theme.zip", name: "Theme file validation") { theme { id } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));

    let upsert = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeFileBodyModes {
          text: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "assets/unicode.txt", body: { type: TEXT, value: "caf\u00e9" } }]) {
            job { id }
            upsertedThemeFiles { filename checksumMd5 size body { content type value } }
            userErrors { field message code }
          }
          base64: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "assets/base64.txt", body: { type: BASE64, value: "aGVsbG8gZnJvbSBiYXNlNjQ=" } }]) {
            job { id }
            upsertedThemeFiles { filename checksumMd5 size body { content type value } }
            userErrors { field message code }
          }
          remote: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "assets/remote.txt", body: { type: URL, value: "https://cdn.example.com/theme-file.txt" } }]) {
            job { id }
            upsertedThemeFiles { filename checksumMd5 size body { content type value } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        upsert.body["data"]["text"]["upsertedThemeFiles"][0],
        json!({"filename": "assets/unicode.txt", "checksumMd5": "07117fe4a1ebd544965dc19573183da2", "size": 5, "body": {"content": "caf\u{00e9}"}})
    );
    assert_eq!(
        upsert.body["data"]["base64"]["upsertedThemeFiles"][0],
        json!({"filename": "assets/base64.txt", "checksumMd5": "c46e1c777b9d4e0b47ea917d2d6d6748", "size": 17, "body": {"content": "hello from base64"}})
    );
    assert_eq!(
        upsert.body["data"]["remote"]["upsertedThemeFiles"][0],
        json!({"filename": "assets/remote.txt", "checksumMd5": "d41d8cd98f00b204e9800998ecf8427e", "size": 0, "body": {"type": "URL", "value": null}})
    );
    for alias in ["text", "base64"] {
        let payload = upsert.body["data"][alias].as_object().unwrap();
        assert!(
            payload.contains_key("job"),
            "{alias} payload should include job"
        );
        assert_eq!(payload["job"], Value::Null);
    }
    let remote_payload = upsert.body["data"]["remote"].as_object().unwrap();
    assert!(
        remote_payload.contains_key("job"),
        "URL-body payload should include job"
    );
    assert!(
        remote_payload["job"]["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("gid://shopify/Job/")),
        "URL-body payload should include a synthetic Job GID: {}",
        remote_payload["job"]
    );

    let conflict = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeFileChecksumConflict($files: [OnlineStoreThemeFilesUpsertFileInput!]!) {
          themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: $files) {
            upsertedThemeFiles { filename checksumMd5 size }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [{
            "filename": "assets/unicode.txt",
            "checksumMd5": "stale-checksum",
            "body": {"type": "TEXT", "value": "changed"}
        }]}),
    ));
    assert_eq!(
        conflict.body["data"]["themeFilesUpsert"],
        json!({"upsertedThemeFiles": [], "userErrors": [{
            "field": ["files", "0", "checksumMd5"],
            "message": "Checksum does not match",
            "code": "CONFLICT"
        }]})
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query RustOnlineStoreThemeFileChecksumConflictRead {
          theme(id: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic") {
            files(first: 10) { nodes { filename checksumMd5 size body { content type value } } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["theme"]["files"]["nodes"][0],
        json!({"filename": "assets/unicode.txt", "checksumMd5": "07117fe4a1ebd544965dc19573183da2", "size": 5, "body": {"content": "caf\u{00e9}"}})
    );
}

#[test]
fn online_store_theme_files_upsert_rejects_validation_regressions_without_staging() {
    let mut proxy = snapshot_proxy();

    proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeFileValidationCreate {
          themeCreate(source: "https://example.com/theme.zip", name: "Theme file validation") { theme { id } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));

    let mutation = r#"
        mutation RustOnlineStoreThemeFileUpsertValidation($files: [OnlineStoreThemeFilesUpsertFileInput!]!) {
          themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: $files) {
            job { id }
            upsertedThemeFiles { filename }
            userErrors { field message code }
          }
        }
    "#;
    let validation = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [
            {"filename": "", "body": {"type": "TEXT", "value": "blank"}},
            {"filename": "evil/path.liquid", "body": {"type": "TEXT", "value": "bad"}},
            {"filename": "_drafts/preview.liquid", "body": {"type": "TEXT", "value": "draft"}},
            {"filename": "assets/dupe.js", "body": {"type": "TEXT", "value": "first"}},
            {"filename": "assets/dupe.js", "body": {"type": "TEXT", "value": "second"}},
            {"filename": "assets/bad-base64.txt", "body": {"type": "BASE64", "value": "not base64"}}
        ]}),
    ));
    assert_eq!(
        validation.body["data"]["themeFilesUpsert"],
        json!({"job": null, "upsertedThemeFiles": [], "userErrors": [
            {"field": ["files", "0", "filename"], "message": "Filename can't be blank", "code": "INVALID"},
            {"field": ["files", "1", "filename"], "message": "Filename is invalid", "code": "INVALID"},
            {"field": ["files", "2", "filename"], "message": "Access denied", "code": "ACCESS_DENIED"},
            {"field": ["files", "4", "filename"], "message": "duplicate-file-input", "code": "INVALID"},
            {"field": ["files", "5", "body"], "message": "invalid-body-input", "code": "INVALID"}
        ]})
    );

    let too_many_files = (0..51)
        .map(|index| {
            json!({
                "filename": format!("assets/file-{index}.txt"),
                "body": {"type": "TEXT", "value": "x"}
            })
        })
        .collect::<Vec<_>>();
    let too_many = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": too_many_files}),
    ));
    assert_eq!(
        too_many.body["data"]["themeFilesUpsert"],
        json!({"job": null, "upsertedThemeFiles": [], "userErrors": [{
            "field": ["files"],
            "message": "Exceeded maximum number of files",
            "code": "INVALID"
        }]})
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query RustOnlineStoreThemeFileRejectedUpsertRead {
          theme(id: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic") {
            files(first: 10) { nodes { filename } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read.body["data"]["theme"]["files"]["nodes"], json!([]));
}

#[test]
fn online_store_theme_files_copy_delete_validate_caps_duplicates_and_required_files() {
    let mut proxy = snapshot_proxy();

    proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeFileCopyDeleteValidationCreate {
          themeCreate(source: "https://example.com/theme.zip", name: "Theme file validation") { theme { id } userErrors { field message code } }
          themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [
            { filename: "assets/source.js", body: { type: TEXT, value: "source" } },
            { filename: "layout/theme.liquid", body: { type: TEXT, value: "<html></html>" } }
          ]) { upsertedThemeFiles { filename } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));

    let copy_mutation = r#"
        mutation RustOnlineStoreThemeFileCopyValidation($files: [ThemeFilesCopyFileInput!]!) {
          themeFilesCopy(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: $files) {
            copiedThemeFiles { filename }
            userErrors { field message code }
          }
        }
    "#;
    let duplicate_copy = proxy.process_request(json_graphql_request(
        copy_mutation,
        json!({"files": [
            {"srcFilename": "assets/source.js", "dstFilename": "assets/copy.js"},
            {"srcFilename": "assets/source.js", "dstFilename": "assets/copy.js"}
        ]}),
    ));
    assert_eq!(
        duplicate_copy.body["data"]["themeFilesCopy"],
        json!({"copiedThemeFiles": [], "userErrors": [{
            "field": ["files", "1", "dstFilename"],
            "message": "duplicate-file-input",
            "code": "INVALID"
        }]})
    );

    let too_many_copies = (0..51)
        .map(|index| {
            json!({
                "srcFilename": "assets/source.js",
                "dstFilename": format!("assets/copy-{index}.js")
            })
        })
        .collect::<Vec<_>>();
    let copy_limit = proxy.process_request(json_graphql_request(
        copy_mutation,
        json!({"files": too_many_copies}),
    ));
    assert_eq!(
        copy_limit.body["data"]["themeFilesCopy"],
        json!({"copiedThemeFiles": [], "userErrors": [{
            "field": ["files"],
            "message": "Exceeded maximum number of files",
            "code": "INVALID"
        }]})
    );

    let delete_mutation = r#"
        mutation RustOnlineStoreThemeFileDeleteValidation($files: [String!]!) {
          themeFilesDelete(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: $files) {
            deletedThemeFiles { filename }
            userErrors { field message code }
          }
        }
    "#;
    let delete_validation = proxy.process_request(json_graphql_request(
        delete_mutation,
        json!({"files": ["assets/source.js", "assets/source.js", "layout/theme.liquid"]}),
    ));
    assert_eq!(
        delete_validation.body["data"]["themeFilesDelete"],
        json!({"deletedThemeFiles": [], "userErrors": [
            {"field": ["files", "1"], "message": "duplicate-file-input", "code": "INVALID"},
            {"field": ["files", "2"], "message": "File is required and can't be deleted", "code": "INVALID"}
        ]})
    );

    let too_many_deletes = (0..101)
        .map(|index| format!("assets/delete-{index}.js"))
        .collect::<Vec<_>>();
    let delete_limit = proxy.process_request(json_graphql_request(
        delete_mutation,
        json!({"files": too_many_deletes}),
    ));
    assert_eq!(
        delete_limit.body["data"]["themeFilesDelete"],
        json!({"deletedThemeFiles": [], "userErrors": [{
            "field": ["files"],
            "message": "Exceeded maximum number of files",
            "code": "INVALID"
        }]})
    );
}

#[test]
fn metaobjects_read_empty_and_lifecycle_state_locally_for_arbitrary_documents() {
    let mut proxy = snapshot_proxy();

    let empty = proxy.process_request(json_graphql_request(
        r#"
        query AnyMetaobjectReadName($id: ID!, $handle: MetaobjectHandleInput!, $type: String!) {
          catalog: metaobjects(type: $type, first: 10) { edges { cursor node { id } } nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          detail: metaobject(id: $id) { id }
          byHandle: metaobjectByHandle(handle: $handle) { id }
        }
        "#,
        json!({
            "id": "gid://shopify/Metaobject/does-not-exist",
            "handle": {"type": "local_article", "handle": "local-entry"},
            "type": "local_article"
        }),
    ));
    assert_eq!(
        empty.body["data"]["catalog"],
        json!({"edges": [], "nodes": [], "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null}})
    );
    assert_eq!(empty.body["data"]["detail"], Value::Null);
    assert_eq!(empty.body["data"]["byHandle"], Value::Null);

    let definition = proxy.process_request(json_graphql_request(
        r#"
        mutation AnyMetaobjectDefinitionCreateName($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id type metaobjectsCount }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"definition": {
            "type": "local_article",
            "name": "Local Article",
            "displayNameKey": "title",
            "fieldDefinitions": [
                {"key": "title", "name": "Title", "type": "single_line_text_field", "required": true},
                {"key": "body", "name": "Body", "type": "multi_line_text_field", "required": false},
                {"key": "summary", "name": "Summary", "type": "single_line_text_field", "required": false}
            ]
        }}),
    ));
    assert_eq!(
        definition.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation AnyMetaobjectCreateName($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject {
              id
              handle
              type
              displayName
              updatedAt
              fields { key type value jsonValue definition { key name required type { name category } } }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"metaobject": {"type": "local_article", "handle": "local-entry", "fields": [{"key": "title", "value": "Local Title"}, {"key": "body", "value": "Local body"}, {"key": "summary", "value": "Local summary"}]}}),
    ));
    let created_id = created.body["data"]["metaobjectCreate"]["metaobject"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(created_id.starts_with("gid://shopify/Metaobject/"));
    assert_eq!(
        created.body["data"]["metaobjectCreate"]["metaobject"]["displayName"],
        json!("Local Title")
    );
    assert_eq!(
        created.body["data"]["metaobjectCreate"]["metaobject"]["handle"],
        json!("local-entry")
    );
    assert_eq!(
        created.body["data"]["metaobjectCreate"]["metaobject"]["type"],
        json!("local_article")
    );
    assert_eq!(
        created.body["data"]["metaobjectCreate"]["metaobject"]["fields"],
        json!([
            {
                "key": "title",
                "type": "single_line_text_field",
                "value": "Local Title",
                "jsonValue": "Local Title",
                "definition": {"key": "title", "name": "Title", "required": true, "type": {"name": "single_line_text_field", "category": "TEXT"}}
            },
            {
                "key": "body",
                "type": "multi_line_text_field",
                "value": "Local body",
                "jsonValue": "Local body",
                "definition": {"key": "body", "name": "Body", "required": false, "type": {"name": "multi_line_text_field", "category": "TEXT"}}
            },
            {
                "key": "summary",
                "type": "single_line_text_field",
                "value": "Local summary",
                "jsonValue": "Local summary",
                "definition": {"key": "summary", "name": "Summary", "required": false, "type": {"name": "single_line_text_field", "category": "TEXT"}}
            }
        ])
    );
    assert_eq!(
        created.body["data"]["metaobjectCreate"]["userErrors"],
        json!([])
    );

    let after_create = proxy.process_request(json_graphql_request(
        r#"
        query AnyDownstreamMetaobjectRead($id: ID!, $handle: MetaobjectHandleInput!, $type: String!) {
          catalog: metaobjects(type: $type, first: 10) { edges { cursor node { id handle type displayName updatedAt } } nodes { id handle type displayName updatedAt } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          detail: metaobject(id: $id) { id handle type displayName updatedAt }
          byHandle: metaobjectByHandle(handle: $handle) { id handle type displayName updatedAt }
        }
        "#,
        json!({
            "id": created_id,
            "handle": {"type": "local_article", "handle": "local-entry"},
            "type": "local_article"
        }),
    ));
    assert_eq!(
        after_create.body["data"]["catalog"]["nodes"][0]["id"],
        created.body["data"]["metaobjectCreate"]["metaobject"]["id"]
    );
    assert_eq!(
        after_create.body["data"]["byHandle"]["displayName"],
        json!("Local Title")
    );

    let deleted = proxy.process_request(json_graphql_request(
        r#"
        mutation AnyMetaobjectDeleteName($id: ID!) {
          metaobjectDelete(id: $id) { deletedId userErrors { field message code elementKey elementIndex } }
        }
        "#,
        json!({"id": created_id}),
    ));
    assert_eq!(
        deleted.body["data"]["metaobjectDelete"],
        json!({"deletedId": created.body["data"]["metaobjectCreate"]["metaobject"]["id"], "userErrors": []})
    );

    let after_delete = proxy.process_request(json_graphql_request(
        r#"
        query AnyReadAfterDelete($id: ID!, $handle: MetaobjectHandleInput!, $type: String!) {
          catalog: metaobjects(type: $type, first: 10) { edges { cursor node { id } } nodes { id } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
          detail: metaobject(id: $id) { id }
          byHandle: metaobjectByHandle(handle: $handle) { id }
        }
        "#,
        json!({
            "id": deleted.body["data"]["metaobjectDelete"]["deletedId"],
            "handle": {"type": "local_article", "handle": "local-entry"},
            "type": "local_article"
        }),
    ));
    assert_eq!(
        after_delete.body["data"]["catalog"],
        json!({"edges": [], "nodes": [], "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null}})
    );
    assert_eq!(after_delete.body["data"]["detail"], Value::Null);
    assert_eq!(after_delete.body["data"]["byHandle"], Value::Null);
}

#[test]
fn metaobjects_connection_filters_and_sorts_staged_records() {
    let mut proxy = snapshot_proxy();
    create_metaobject_definition_for_test(
        &mut proxy,
        "filter_sort_article",
        vec![
            json!({
                "key": "title",
                "name": "Title",
                "type": "single_line_text_field",
                "required": true
            }),
            json!({
                "key": "subtitle",
                "name": "Subtitle",
                "type": "single_line_text_field",
                "required": false
            }),
        ],
    );

    let create_metaobject =
        |proxy: &mut DraftProxy, handle: &str, title: &str, subtitle: &str| -> String {
            let response = proxy.process_request(json_graphql_request(
                r#"
            mutation CreateFilterSortMetaobject($metaobject: MetaobjectCreateInput!) {
              metaobjectCreate(metaobject: $metaobject) {
                metaobject { id }
                userErrors { field message code elementKey elementIndex }
              }
            }
            "#,
                json!({"metaobject": {
                    "type": "filter_sort_article",
                    "handle": handle,
                    "fields": [
                        {"key": "title", "value": title},
                        {"key": "subtitle", "value": subtitle}
                    ]
                }}),
            ));
            assert_eq!(
                response.body["data"]["metaobjectCreate"]["userErrors"],
                json!([])
            );
            response.body["data"]["metaobjectCreate"]["metaobject"]["id"]
                .as_str()
                .unwrap()
                .to_string()
        };

    let charlie_id = create_metaobject(&mut proxy, "charlie-handle", "Charlie", "Lake story");
    let alpha_id = create_metaobject(&mut proxy, "alpha-handle", "Alpha", "River note");
    let bravo_id = create_metaobject(&mut proxy, "bravo-handle", "Bravo", "Hill note");

    let read_connection =
        |proxy: &mut DraftProxy, query: Value, sort_key: Value, reverse: bool| -> Vec<String> {
            let response = proxy.process_request(json_graphql_request(
                r#"
                query ReadFilterSortMetaobjects(
                  $type: String!
                  $query: String
                  $sortKey: String
                  $reverse: Boolean
                ) {
                  metaobjects(
                    type: $type
                    first: 10
                    query: $query
                    sortKey: $sortKey
                    reverse: $reverse
                  ) {
                    nodes {
                      id
                      handle
                      type
                      displayName
                      updatedAt
                      fields { key value jsonValue }
                    }
                  }
                }
                "#,
                json!({
                    "type": "filter_sort_article",
                    "query": query,
                    "sortKey": sort_key,
                    "reverse": reverse
                }),
            ));
            response.body["data"]["metaobjects"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|node| node["id"].as_str().unwrap().to_string())
                .collect()
        };

    assert_eq!(
        read_connection(&mut proxy, json!("display_name:Alpha"), Value::Null, false),
        vec![alpha_id.clone()]
    );
    assert_eq!(
        read_connection(
            &mut proxy,
            json!("fields.subtitle:lake"),
            Value::Null,
            false
        ),
        vec![charlie_id.clone()]
    );
    assert_eq!(
        read_connection(&mut proxy, json!("handle:bravo-handle"), Value::Null, false),
        vec![bravo_id.clone()]
    );

    let alpha_tail = alpha_id.rsplit('/').next().unwrap();
    assert_eq!(
        read_connection(
            &mut proxy,
            json!(format!("id:>={alpha_tail}")),
            Value::Null,
            false
        ),
        vec![alpha_id.clone(), bravo_id.clone()]
    );
    assert_eq!(
        read_connection(
            &mut proxy,
            json!("updated_at:<2026-01-01T00:00:00Z"),
            Value::Null,
            false
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        read_connection(
            &mut proxy,
            json!("unknown_filter:value"),
            Value::Null,
            false
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        read_connection(&mut proxy, Value::Null, json!("display_name"), true),
        vec![charlie_id.clone(), bravo_id.clone(), alpha_id.clone()]
    );
    assert_eq!(
        read_connection(&mut proxy, Value::Null, json!("id"), true),
        vec![bravo_id, alpha_id, charlie_id]
    );
}

#[test]
fn metaobject_create_rejects_duplicate_field_keys() {
    let mut proxy = snapshot_proxy();

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation ArbitraryDuplicateCreateDocument($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) { metaobject { id displayName fields { key value } } userErrors { field message code elementKey elementIndex } }
        }
        "#,
        json!({
            "metaobject": {
                "type": "codex_update_errors_1778040780683",
                "fields": [
                    {"key": "title", "value": "First 1778040780683"},
                    {"key": "title", "value": ""},
                    {"key": "body", "value": "Body 1778040780683"}
                ]
            }
        }),
    ));

    assert_eq!(
        created.body["data"]["metaobjectCreate"],
        json!({
            "metaobject": null,
            "userErrors": [
                {
                    "field": ["metaobject", "fields", "1"],
                    "message": "Field \"title\" duplicates other inputs",
                    "code": "DUPLICATE_FIELD_INPUT",
                    "elementKey": "title",
                    "elementIndex": null
                },
                {
                    "field": ["metaobject", "fields", "1"],
                    "message": "Title can't be blank",
                    "code": "OBJECT_FIELD_REQUIRED",
                    "elementKey": "title",
                    "elementIndex": null
                }
            ]
        })
    );

    let after_rejected_create = proxy.process_request(json_graphql_request(
        r#"
        query ArbitraryRejectedCreateRead($type: String!) {
          metaobjects(type: $type, first: 10) { nodes { id } }
        }
        "#,
        json!({"type": "codex_update_errors_1778040780683"}),
    ));
    assert_eq!(
        after_rejected_create.body["data"]["metaobjects"]["nodes"],
        json!([])
    );
}

#[test]
fn metaobject_definition_field_key_validation_matches_shopify_length_and_case_rules() {
    let mut proxy = snapshot_proxy();

    let create_definition = r#"
        mutation CreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id type fieldDefinitions { key } }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let update_definition = r#"
        mutation UpdateDefinition($id: ID!, $definition: MetaobjectDefinitionUpdateInput!) {
          metaobjectDefinitionUpdate(id: $id, definition: $definition) {
            metaobjectDefinition { id fieldDefinitions { key } }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let field_definition = |key: String| {
        json!({
            "key": key,
            "name": "Field",
            "type": "single_line_text_field",
            "required": false
        })
    };

    let oversized_key = "a".repeat(65);
    let oversized_create = proxy.process_request(json_graphql_request(
        create_definition,
        json!({"definition": {
            "type": "field_key_length_create",
            "name": "Field Key Length Create",
            "fieldDefinitions": [field_definition(oversized_key.clone())]
        }}),
    ));
    assert_eq!(
        oversized_create.body["data"]["metaobjectDefinitionCreate"],
        json!({
            "metaobjectDefinition": null,
            "userErrors": [{
                "field": ["definition", "fieldDefinitions", "0"],
                "message": "Key is too long (maximum is 64 characters)",
                "code": "TOO_LONG",
                "elementKey": oversized_key,
                "elementIndex": null
            }]
        })
    );
    let rejected_create_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadRejectedDefinition($type: String!) {
          metaobjectDefinitionByType(type: $type) { id }
        }
        "#,
        json!({"type": "field_key_length_create"}),
    ));
    assert_eq!(
        rejected_create_read.body["data"]["metaobjectDefinitionByType"],
        Value::Null
    );

    let uppercase_create = proxy.process_request(json_graphql_request(
        create_definition,
        json!({"definition": {
            "type": "field_key_case_create",
            "name": "Field Key Case Create",
            "fieldDefinitions": [field_definition("myField".to_string())]
        }}),
    ));
    assert_eq!(
        uppercase_create.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        uppercase_create.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"]
            ["fieldDefinitions"][0]["key"],
        json!("myField")
    );

    let base_definition = proxy.process_request(json_graphql_request(
        create_definition,
        json!({"definition": {
            "type": "field_key_update_rules",
            "name": "Field Key Update Rules",
            "fieldDefinitions": [field_definition("title".to_string())]
        }}),
    ));
    assert_eq!(
        base_definition.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );
    let definition_id = base_definition.body["data"]["metaobjectDefinitionCreate"]
        ["metaobjectDefinition"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let oversized_update = proxy.process_request(json_graphql_request(
        update_definition,
        json!({"id": definition_id, "definition": {
            "fieldDefinitions": [{"create": field_definition(oversized_key.clone())}]
        }}),
    ));
    assert_eq!(
        oversized_update.body["data"]["metaobjectDefinitionUpdate"],
        json!({
            "metaobjectDefinition": null,
            "userErrors": [{
                "field": ["definition", "fieldDefinitions", "0", "create"],
                "message": "Key is too long (maximum is 64 characters)",
                "code": "TOO_LONG",
                "elementKey": oversized_key,
                "elementIndex": null
            }]
        })
    );

    let uppercase_update = proxy.process_request(json_graphql_request(
        update_definition,
        json!({"id": definition_id, "definition": {
            "fieldDefinitions": [{"create": field_definition("Spec_2".to_string())}]
        }}),
    ));
    assert_eq!(
        uppercase_update.body["data"]["metaobjectDefinitionUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        uppercase_update.body["data"]["metaobjectDefinitionUpdate"]["metaobjectDefinition"]
            ["fieldDefinitions"][1]["key"],
        json!("Spec_2")
    );
}

#[test]
fn metaobject_definition_create_limits_field_and_admin_filterable_counts() {
    let mut proxy = snapshot_proxy();

    let create_definition = r#"
        mutation CreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id fieldDefinitions { key capabilities { adminFilterable { enabled } } } }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let admin_filterable_field = |index: usize| {
        json!({
            "key": format!("field_{index:03}"),
            "name": format!("Field {index}"),
            "type": "single_line_text_field",
            "capabilities": { "adminFilterable": { "enabled": true } }
        })
    };

    let accepted_fields = (0..40).map(admin_filterable_field).collect::<Vec<_>>();
    let accepted = proxy.process_request(json_graphql_request(
        create_definition,
        json!({"definition": {
            "type": "admin_filterable_field_limit_40",
            "name": "Admin Filterable Field Limit 40",
            "displayNameKey": "field_000",
            "fieldDefinitions": accepted_fields
        }}),
    ));
    assert_eq!(
        accepted.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        accepted.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"]
            ["fieldDefinitions"]
            .as_array()
            .unwrap()
            .len(),
        40
    );

    let rejected_fields = (0..41).map(admin_filterable_field).collect::<Vec<_>>();
    let rejected = proxy.process_request(json_graphql_request(
        create_definition,
        json!({"definition": {
            "type": "admin_filterable_field_limit_41",
            "name": "Admin Filterable Field Limit 41",
            "displayNameKey": "field_000",
            "fieldDefinitions": rejected_fields
        }}),
    ));
    assert_eq!(
        rejected.body["data"]["metaobjectDefinitionCreate"],
        json!({
            "metaobjectDefinition": null,
            "userErrors": [
                {
                    "field": ["definition", "fieldDefinitions"],
                    "message": "Maximum 40 fields per metaobject definition",
                    "code": "INVALID",
                    "elementKey": null,
                    "elementIndex": null
                },
                {
                    "field": ["definition", "fieldDefinitions"],
                    "message": "Maximum 40 admin filterable fields per metaobject definition",
                    "code": "INVALID",
                    "elementKey": null,
                    "elementIndex": null
                }
            ]
        })
    );
}

#[test]
fn metaobject_definition_create_reports_captured_shop_definition_limit() {
    let mut proxy = snapshot_proxy();

    let create_definition = r#"
        mutation CreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let field_definition = json!({
        "key": "title",
        "name": "Title",
        "type": "single_line_text_field"
    });

    for index in 0..128 {
        let created = proxy.process_request(json_graphql_request(
            create_definition,
            json!({"definition": {
                "type": format!("definition_limit_{index:03}"),
                "name": format!("Definition Limit {index}"),
                "displayNameKey": "title",
                "fieldDefinitions": [field_definition.clone()]
            }}),
        ));
        assert_eq!(
            created.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
            json!([]),
            "definition {index} should be accepted"
        );
    }

    let rejected = proxy.process_request(json_graphql_request(
        create_definition,
        json!({"definition": {
            "type": "definition_limit_128",
            "name": "Definition Limit 128",
            "displayNameKey": "title",
            "fieldDefinitions": [field_definition]
        }}),
    ));
    assert_eq!(
        rejected.body["data"]["metaobjectDefinitionCreate"],
        json!({
            "metaobjectDefinition": null,
            "userErrors": [{
                "field": ["definition"],
                "message": "Total definition count exceeds the limit of 128",
                "code": "MAX_DEFINITIONS_EXCEEDED",
                "elementKey": null,
                "elementIndex": null
            }]
        })
    );
}

#[test]
fn metaobject_definition_app_type_uses_request_api_client_id() {
    let mut proxy = snapshot_proxy();
    let create_definition = r#"
        mutation CreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id type }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let definition_input = json!({
        "definition": {
            "type": "$app:settings_box",
            "name": "App Settings Box",
            "fieldDefinitions": [{
                "key": "title",
                "name": "Title",
                "type": "single_line_text_field",
                "required": false
            }]
        }
    });

    let mut create_request = json_graphql_request(create_definition, definition_input.clone());
    create_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "999999999999".to_string(),
    );
    let create = proxy.process_request(create_request);
    assert_eq!(
        create.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );
    let created_definition =
        &create.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"];
    assert_eq!(
        created_definition["type"],
        json!("app--999999999999--settings_box")
    );

    let mut read_request = json_graphql_request(
        r#"
        query ReadDefinitionByType($type: String!) {
          metaobjectDefinitionByType(type: $type) { id type }
        }
        "#,
        json!({"type": "$app:settings_box"}),
    );
    read_request.headers.insert(
        "x-shopify-draft-proxy-api-client-id".to_string(),
        "999999999999".to_string(),
    );
    let read = proxy.process_request(read_request);
    assert_eq!(
        read.body["data"]["metaobjectDefinitionByType"]["type"],
        json!("app--999999999999--settings_box")
    );

    let missing_identity =
        proxy.process_request(json_graphql_request(create_definition, definition_input));
    assert_eq!(
        missing_identity.body["data"]["metaobjectDefinitionCreate"],
        json!({
            "metaobjectDefinition": null,
            "userErrors": [{
                "field": ["definition", "type"],
                "message": "API client identity is required to resolve or authorize app-reserved namespaces and types.",
                "code": "NOT_AUTHORIZED",
                "elementKey": null,
                "elementIndex": null
            }]
        })
    );
}

#[test]
fn metaobject_definition_update_validates_field_create_keys_and_display_name_key() {
    let mut proxy = snapshot_proxy();

    let create_definition = r#"
        mutation CreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let update_definition = r#"
        mutation UpdateDefinition($id: ID!, $definition: MetaobjectDefinitionUpdateInput!) {
          metaobjectDefinitionUpdate(id: $id, definition: $definition) {
            metaobjectDefinition { id fieldDefinitions { key } displayNameKey }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;

    let create_field = |key: String| {
        json!({
            "key": key,
            "name": "Field",
            "type": "single_line_text_field",
            "required": false
        })
    };
    let create_local_definition =
        |proxy: &mut DraftProxy, meta_type: &str, field_definitions: Vec<Value>| -> String {
            let response = proxy.process_request(json_graphql_request(
                create_definition,
                json!({"definition": {
                    "type": meta_type,
                    "name": meta_type,
                    "displayNameKey": field_definitions[0]["key"],
                    "fieldDefinitions": field_definitions
                }}),
            ));
            assert_eq!(
                response.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
                json!([])
            );
            response.body["data"]["metaobjectDefinitionCreate"]["metaobjectDefinition"]["id"]
                .as_str()
                .unwrap()
                .to_string()
        };

    let reserved_id = create_local_definition(
        &mut proxy,
        "update_reserved_field_key",
        vec![create_field("title".to_string())],
    );
    for reserved_key in ["id", "handle", "system", "metafields"] {
        let reserved = proxy.process_request(json_graphql_request(
            update_definition,
            json!({"id": reserved_id, "definition": {
                "fieldDefinitions": [{"create": create_field(reserved_key.to_string())}]
            }}),
        ));
        assert_eq!(
            reserved.body["data"]["metaobjectDefinitionUpdate"],
            json!({
                "metaobjectDefinition": null,
                "userErrors": [{
                    "field": ["definition", "fieldDefinitions", "0"],
                    "message": format!("The name \"{reserved_key}\" is reserved for system use"),
                    "code": "RESERVED_NAME",
                    "elementKey": reserved_key,
                    "elementIndex": null
                }]
            })
        );
    }

    let duplicate_id = create_local_definition(
        &mut proxy,
        "update_duplicate_field_key",
        vec![create_field("title".to_string())],
    );
    let duplicate = proxy.process_request(json_graphql_request(
        update_definition,
        json!({"id": duplicate_id, "definition": {
            "fieldDefinitions": [
                {"create": create_field("new_field".to_string())},
                {"create": create_field("new_field".to_string())}
            ]
        }}),
    ));
    assert_eq!(
        duplicate.body["data"]["metaobjectDefinitionUpdate"],
        json!({
            "metaobjectDefinition": null,
            "userErrors": [{
                "field": ["definition", "fieldDefinitions", "1"],
                "message": "Field \"new_field\" duplicates other inputs",
                "code": "DUPLICATE_FIELD_INPUT",
                "elementKey": "new_field",
                "elementIndex": null
            }]
        })
    );

    let max_fields = (0..40)
        .map(|index| create_field(format!("field_{index:02}")))
        .collect::<Vec<_>>();
    let max_id = create_local_definition(&mut proxy, "update_too_many_fields", max_fields);
    let too_many = proxy.process_request(json_graphql_request(
        update_definition,
        json!({"id": max_id, "definition": {
            "fieldDefinitions": [{"create": create_field("field_40".to_string())}]
        }}),
    ));
    assert_eq!(
        too_many.body["data"]["metaobjectDefinitionUpdate"],
        json!({
            "metaobjectDefinition": null,
            "userErrors": [{
                "field": ["definition", "fieldDefinitions"],
                "message": "Maximum 40 fields per metaobject definition",
                "code": "INVALID",
                "elementKey": null,
                "elementIndex": null
            }]
        })
    );

    let display_id = create_local_definition(
        &mut proxy,
        "update_display_name_key_missing",
        vec![create_field("title".to_string())],
    );
    let missing_display_key = proxy.process_request(json_graphql_request(
        update_definition,
        json!({"id": display_id, "definition": {"displayNameKey": "ghost"}}),
    ));
    assert_eq!(
        missing_display_key.body["data"]["metaobjectDefinitionUpdate"],
        json!({
            "metaobjectDefinition": null,
            "userErrors": [{
                "field": ["definition", "displayNameKey"],
                "message": "Field definition \"ghost\" does not exist",
                "code": "UNDEFINED_OBJECT_FIELD",
                "elementKey": null,
                "elementIndex": null
            }]
        })
    );

    let new_display_id = create_local_definition(
        &mut proxy,
        "update_display_name_key_created",
        vec![create_field("title".to_string())],
    );
    let created_display_key = proxy.process_request(json_graphql_request(
        update_definition,
        json!({"id": new_display_id, "definition": {
            "displayNameKey": "subtitle",
            "fieldDefinitions": [{"create": create_field("subtitle".to_string())}]
        }}),
    ));
    assert_eq!(
        created_display_key.body["data"]["metaobjectDefinitionUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        created_display_key.body["data"]["metaobjectDefinitionUpdate"]["metaobjectDefinition"]
            ["displayNameKey"],
        json!("subtitle")
    );

    let deleted_display_id = create_local_definition(
        &mut proxy,
        "update_display_name_key_deleted",
        vec![
            create_field("title".to_string()),
            create_field("summary".to_string()),
        ],
    );
    let deleted_display_key = proxy.process_request(json_graphql_request(
        update_definition,
        json!({"id": deleted_display_id, "definition": {
            "displayNameKey": "title",
            "fieldDefinitions": [{"delete": {"key": "title"}}]
        }}),
    ));
    assert_eq!(
        deleted_display_key.body["data"]["metaobjectDefinitionUpdate"],
        json!({
            "metaobjectDefinition": null,
            "userErrors": [{
                "field": ["definition", "displayNameKey"],
                "message": "Field definition \"title\" does not exist",
                "code": "UNDEFINED_OBJECT_FIELD",
                "elementKey": null,
                "elementIndex": null
            }]
        })
    );
}

#[test]
fn metaobject_entry_lifecycle_dispatches_by_root_field_and_definition_state() {
    let mut proxy = snapshot_proxy();

    let definition = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id type displayNameKey metaobjectsCount fieldDefinitions { key name required type { name category } } capabilities { publishable { enabled } } }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"definition": {
            "type": "ticket_metaobject_type",
            "name": "Ticket Metaobject",
            "displayNameKey": "heading",
            "capabilities": {"publishable": {"enabled": true}},
            "fieldDefinitions": [
                {"key": "heading", "name": "Heading", "type": "single_line_text_field", "required": true},
                {"key": "rank", "name": "Rank", "type": "number_integer", "required": false},
                {"key": "body", "name": "Body", "type": "multi_line_text_field", "required": false}
            ]
        }}),
    ));
    assert_eq!(
        definition.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMetaobject($metaobject: MetaobjectCreateInput!) {
          created: metaobjectCreate(metaobject: $metaobject) {
            metaobject {
              id
              handle
              type
              displayName
              capabilities { publishable { status } }
              fields { key type value jsonValue definition { key name required type { name category } } }
              headingField: field(key: "heading") { key value jsonValue }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"metaobject": {
            "type": "ticket_metaobject_type",
            "values": {"heading": "Normal Operation", "rank": "7", "body": "Projected body"}
        }}),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["created"]["userErrors"], json!([]));
    let created = &create.body["data"]["created"]["metaobject"];
    let created_id = created["id"].as_str().unwrap().to_string();
    let created_handle = created["handle"].as_str().unwrap().to_string();
    assert_core_metaobject_auto_handle(&created_handle, "ticket-metaobject-type-");
    assert_eq!(created["displayName"], json!("Normal Operation"));
    assert_eq!(
        created["capabilities"]["publishable"]["status"],
        json!("DRAFT")
    );
    assert_eq!(created["fields"][1]["jsonValue"], json!(7));
    assert_eq!(
        created["headingField"],
        json!({"key": "heading", "value": "Normal Operation", "jsonValue": "Normal Operation"})
    );

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateAnotherMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id handle displayName }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"metaobject": {
            "type": "ticket_metaobject_type",
            "fields": [{"key": "heading", "value": "Normal Operation"}]
        }}),
    ));
    let duplicate_metaobject = &duplicate.body["data"]["metaobjectCreate"]["metaobject"];
    let duplicate_handle = duplicate_metaobject["handle"].as_str().unwrap();
    assert_core_metaobject_auto_handle(duplicate_handle, "ticket-metaobject-type-");
    assert_ne!(duplicate_handle, created_handle);
    assert_eq!(
        duplicate_metaobject["displayName"],
        json!("Normal Operation")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadMetaobject($id: ID!, $handle: MetaobjectHandleInput!, $type: String!) {
          detailAlias: metaobject(id: $id) { id handle displayName definition { type metaobjectsCount } }
          handleAlias: metaobjectByHandle(handle: $handle) { id handle displayName }
          catalogAlias: metaobjects(type: $type, first: 10) {
            nodes { id handle displayName }
            edges { cursor node { id handle } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          definitionAlias: metaobjectDefinitionByType(type: $type) { type metaobjectsCount }
        }
        "#,
        json!({
            "id": created_id,
            "handle": {"type": "ticket_metaobject_type", "handle": created_handle},
            "type": "ticket_metaobject_type"
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["detailAlias"]["displayName"],
        json!("Normal Operation")
    );
    assert_eq!(read.body["data"]["handleAlias"]["id"], created["id"]);
    assert_eq!(
        read.body["data"]["catalogAlias"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        read.body["data"]["definitionAlias"]["metaobjectsCount"],
        json!(2)
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteMetaobject($id: ID!) {
          removed: metaobjectDelete(id: $id) {
            deletedId
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"id": created_id}),
    ));
    assert_eq!(delete.body["data"]["removed"]["deletedId"], created["id"]);
    assert_eq!(delete.body["data"]["removed"]["userErrors"], json!([]));

    let after_delete = proxy.process_request(json_graphql_request(
        r#"
        query ReadAfterDelete($id: ID!, $handle: MetaobjectHandleInput!, $type: String!) {
          detail: metaobject(id: $id) { id }
          byHandle: metaobjectByHandle(handle: $handle) { id }
          catalog: metaobjects(type: $type, first: 10) { nodes { id } }
          definition: metaobjectDefinitionByType(type: $type) { metaobjectsCount }
        }
        "#,
        json!({
            "id": created["id"],
            "handle": {"type": "ticket_metaobject_type", "handle": created["handle"]},
            "type": "ticket_metaobject_type"
        }),
    ));
    assert_eq!(after_delete.body["data"]["detail"], Value::Null);
    assert_eq!(after_delete.body["data"]["byHandle"], Value::Null);
    assert_eq!(
        after_delete.body["data"]["catalog"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        after_delete.body["data"]["definition"]["metaobjectsCount"],
        json!(1)
    );
}

#[test]
fn metaobject_entry_online_store_template_suffix_persists_across_local_lifecycle() {
    let mut proxy = snapshot_proxy();

    let definition = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateOnlineStoreDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id type capabilities { onlineStore { enabled } } }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"definition": {
            "type": "online_store_suffix_test",
            "name": "Online Store Suffix Test",
            "displayNameKey": "title",
            "access": {"storefront": "PUBLIC_READ"},
            "capabilities": {"onlineStore": {"enabled": true}},
            "fieldDefinitions": [
                {"key": "title", "name": "Title", "type": "single_line_text_field", "required": true},
                {"key": "body", "name": "Body", "type": "single_line_text_field", "required": false}
            ]
        }}),
    ));
    assert_eq!(
        definition.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );

    let create_query = r#"
        mutation CreateMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject {
              id
              handle
              capabilities { onlineStore { templateSuffix } }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let update_query = r#"
        mutation UpdateMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject {
              id
              handle
              fields { key value }
              capabilities { onlineStore { templateSuffix } }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let upsert_query = r#"
        mutation UpsertMetaobject($handle: MetaobjectHandleInput!, $metaobject: MetaobjectUpsertInput!) {
          metaobjectUpsert(handle: $handle, metaobject: $metaobject) {
            metaobject {
              id
              handle
              fields { key value }
              capabilities { onlineStore { templateSuffix } }
            }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let read_query = r#"
        query ReadMetaobject($id: ID!, $handle: MetaobjectHandleInput!) {
          detail: metaobject(id: $id) { capabilities { onlineStore { templateSuffix } } }
          byHandle: metaobjectByHandle(handle: $handle) { capabilities { onlineStore { templateSuffix } } }
        }
        "#;

    let omitted = proxy.process_request(json_graphql_request(
        create_query,
        json!({"metaobject": {
            "type": "online_store_suffix_test",
            "handle": "omitted",
            "fields": [{"key": "title", "value": "Omitted"}]
        }}),
    ));
    assert_eq!(
        omitted.body["data"]["metaobjectCreate"]["metaobject"]["capabilities"]["onlineStore"]
            ["templateSuffix"],
        Value::Null
    );

    let empty = proxy.process_request(json_graphql_request(
        create_query,
        json!({"metaobject": {
            "type": "online_store_suffix_test",
            "handle": "empty",
            "capabilities": {"onlineStore": {"templateSuffix": ""}},
            "fields": [{"key": "title", "value": "Empty"}]
        }}),
    ));
    assert_eq!(
        empty.body["data"]["metaobjectCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        empty.body["data"]["metaobjectCreate"]["metaobject"]["capabilities"]["onlineStore"]
            ["templateSuffix"],
        json!("")
    );

    let custom = proxy.process_request(json_graphql_request(
        create_query,
        json!({"metaobject": {
            "type": "online_store_suffix_test",
            "handle": "custom",
            "capabilities": {"onlineStore": {"templateSuffix": "custom"}},
            "fields": [{"key": "title", "value": "Custom"}, {"key": "body", "value": "Original"}]
        }}),
    ));
    assert_eq!(
        custom.body["data"]["metaobjectCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        custom.body["data"]["metaobjectCreate"]["metaobject"]["capabilities"]["onlineStore"]
            ["templateSuffix"],
        json!("custom")
    );
    let custom_id = custom.body["data"]["metaobjectCreate"]["metaobject"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let custom_handle = custom.body["data"]["metaobjectCreate"]["metaobject"]["handle"]
        .as_str()
        .unwrap()
        .to_string();

    let read_custom = proxy.process_request(json_graphql_request(
        read_query,
        json!({
            "id": custom_id,
            "handle": {"type": "online_store_suffix_test", "handle": custom_handle}
        }),
    ));
    assert_eq!(
        read_custom.body["data"]["detail"]["capabilities"]["onlineStore"]["templateSuffix"],
        json!("custom")
    );
    assert_eq!(
        read_custom.body["data"]["byHandle"]["capabilities"]["onlineStore"]["templateSuffix"],
        json!("custom")
    );

    let unrelated_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": custom.body["data"]["metaobjectCreate"]["metaobject"]["id"], "metaobject": {
            "fields": [{"key": "body", "value": "Changed"}]
        }}),
    ));
    assert_eq!(
        unrelated_update.body["data"]["metaobjectUpdate"]["metaobject"]["capabilities"]
            ["onlineStore"]["templateSuffix"],
        json!("custom")
    );

    let explicit_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": custom.body["data"]["metaobjectCreate"]["metaobject"]["id"], "metaobject": {
            "capabilities": {"onlineStore": {"templateSuffix": "updated"}}
        }}),
    ));
    assert_eq!(
        explicit_update.body["data"]["metaobjectUpdate"]["metaobject"]["capabilities"]
            ["onlineStore"]["templateSuffix"],
        json!("updated")
    );

    let upsert_create = proxy.process_request(json_graphql_request(
        upsert_query,
        json!({
            "handle": {"type": "online_store_suffix_test", "handle": "upserted"},
            "metaobject": {
                "capabilities": {"onlineStore": {"templateSuffix": "upserted"}},
                "fields": [{"key": "title", "value": "Upserted"}, {"key": "body", "value": "Original"}]
            }
        }),
    ));
    assert_eq!(
        upsert_create.body["data"]["metaobjectUpsert"]["userErrors"],
        json!([])
    );
    assert_eq!(
        upsert_create.body["data"]["metaobjectUpsert"]["metaobject"]["capabilities"]["onlineStore"]
            ["templateSuffix"],
        json!("upserted")
    );

    let upsert_update_preserve = proxy.process_request(json_graphql_request(
        upsert_query,
        json!({
            "handle": {"type": "online_store_suffix_test", "handle": "upserted"},
            "metaobject": {"fields": [{"key": "body", "value": "Upsert changed"}]}
        }),
    ));
    assert_eq!(
        upsert_update_preserve.body["data"]["metaobjectUpsert"]["metaobject"]["capabilities"]
            ["onlineStore"]["templateSuffix"],
        json!("upserted")
    );

    let upsert_update_empty = proxy.process_request(json_graphql_request(
        upsert_query,
        json!({
            "handle": {"type": "online_store_suffix_test", "handle": "upserted"},
            "metaobject": {"capabilities": {"onlineStore": {"templateSuffix": ""}}}
        }),
    ));
    assert_eq!(
        upsert_update_empty.body["data"]["metaobjectUpsert"]["metaobject"]["capabilities"]
            ["onlineStore"]["templateSuffix"],
        json!("")
    );
}

#[test]
fn metaobject_auto_handles_and_fallback_display_names_follow_core_shapes() {
    let mut proxy = snapshot_proxy();

    let definition = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id type displayNameKey fieldDefinitions { key } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"definition": {
            "type": "auto_handle_test",
            "name": "Auto Handle Test",
            "fieldDefinitions": [
                {"key": "body", "name": "Body", "type": "multi_line_text_field", "required": false}
            ]
        }}),
    ));
    assert_eq!(
        definition.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );

    let auto_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id handle type displayName }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"metaobject": {
            "type": "auto_handle_test",
            "fields": [{"key": "body", "value": "Generated display name fallback"}]
        }}),
    ));
    assert_eq!(
        auto_create.body["data"]["metaobjectCreate"]["userErrors"],
        json!([])
    );
    let auto_metaobject = &auto_create.body["data"]["metaobjectCreate"]["metaobject"];
    let auto_handle = auto_metaobject["handle"].as_str().unwrap();
    assert_core_metaobject_auto_handle(auto_handle, "auto-handle-test-");
    let auto_code = auto_handle.rsplit_once('-').unwrap().1.to_ascii_uppercase();
    assert_eq!(
        auto_metaobject["displayName"],
        json!(format!("Auto Handle Test #{auto_code}"))
    );

    let explicit_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateExplicit($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { handle displayName }
            userErrors { field message code }
          }
        }
        "#,
        json!({"metaobject": {
            "type": "auto_handle_test",
            "handle": "MyHandle",
            "fields": [{"key": "body", "value": "Explicit MyHandle"}]
        }}),
    ));
    assert_eq!(
        explicit_create.body["data"]["metaobjectCreate"]["metaobject"],
        json!({"handle": "myhandle", "displayName": "My Handle"})
    );

    let conflict_create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateConflict($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { handle displayName }
            userErrors { field message code }
          }
        }
        "#,
        json!({"metaobject": {
            "type": "auto_handle_test",
            "handle": "myhandle",
            "fields": [{"key": "body", "value": "Explicit myhandle"}]
        }}),
    ));
    assert_eq!(
        conflict_create.body["data"]["metaobjectCreate"]["metaobject"],
        json!({"handle": "myhandle-1", "displayName": "Myhandle 1"})
    );

    let upsert_create = proxy.process_request(json_graphql_request(
        r#"
        mutation UpsertCreate($handle: MetaobjectHandleInput!, $metaobject: MetaobjectUpsertInput!) {
          metaobjectUpsert(handle: $handle, metaobject: $metaobject) {
            metaobject { id handle displayName }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "handle": {"type": "auto_handle_test", "handle": "UpsertHandle"},
            "metaobject": {"fields": [{"key": "body", "value": "Upsert create"}]}
        }),
    ));
    assert_eq!(
        upsert_create.body["data"]["metaobjectUpsert"]["userErrors"],
        json!([])
    );
    let upsert_id = upsert_create.body["data"]["metaobjectUpsert"]["metaobject"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        upsert_create.body["data"]["metaobjectUpsert"]["metaobject"]["handle"],
        json!("upserthandle")
    );
    assert_eq!(
        upsert_create.body["data"]["metaobjectUpsert"]["metaobject"]["displayName"],
        json!("Upsert Handle")
    );

    let upsert_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpsertUpdate($handle: MetaobjectHandleInput!, $metaobject: MetaobjectUpsertInput!) {
          metaobjectUpsert(handle: $handle, metaobject: $metaobject) {
            metaobject { id handle displayName }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "handle": {"type": "auto_handle_test", "handle": "UpsertHandle"},
            "metaobject": {"fields": [{"key": "body", "value": "Upsert update"}]}
        }),
    ));
    assert_eq!(
        upsert_update.body["data"]["metaobjectUpsert"]["metaobject"],
        json!({"id": upsert_id, "handle": "upserthandle", "displayName": "Upsert Handle"})
    );
}

#[test]
fn metaobject_create_and_upsert_validate_explicit_handles_before_staging() {
    let mut proxy = snapshot_proxy();
    create_metaobject_definition_for_test(
        &mut proxy,
        "handle_validation_type",
        vec![
            json!({"key": "title", "name": "Title", "type": "single_line_text_field", "required": false}),
        ],
    );

    let create_query = r#"
        mutation CreateMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id handle type displayName }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let update_query = r#"
        mutation UpdateMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject { id handle type displayName }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let upsert_query = r#"
        mutation UpsertMetaobject($handle: MetaobjectHandleInput!, $metaobject: MetaobjectUpsertInput!) {
          metaobjectUpsert(handle: $handle, metaobject: $metaobject) {
            metaobject { id handle type displayName }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let read_query = r#"
        query ReadHandleValidationState($type: String!, $handle: MetaobjectHandleInput!) {
          definition: metaobjectDefinitionByType(type: $type) { metaobjectsCount }
          entries: metaobjects(type: $type, first: 10) { nodes { id handle } }
          byHandle: metaobjectByHandle(handle: $handle) { id handle }
        }
        "#;

    let invalid_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({"metaobject": {
            "type": "handle_validation_type",
            "handle": "hello world!",
            "fields": [{"key": "title", "value": "Handle hello world!"}]
        }}),
    ));
    assert_eq!(
        invalid_create.body["data"]["metaobjectCreate"],
        json!({
            "metaobject": null,
            "userErrors": [{
                "field": ["metaobject", "handle"],
                "message": "Handle is invalid",
                "code": "INVALID",
                "elementKey": null,
                "elementIndex": null
            }]
        })
    );

    let too_long = "x".repeat(256);
    let too_long_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({"metaobject": {
            "type": "handle_validation_type",
            "handle": too_long,
            "fields": [{"key": "title", "value": "Handle too long"}]
        }}),
    ));
    assert_eq!(
        too_long_create.body["data"]["metaobjectCreate"],
        json!({
            "metaobject": null,
            "userErrors": [{
                "field": ["metaobject", "handle"],
                "message": "Handle is too long (maximum is 255 characters)",
                "code": "TOO_LONG",
                "elementKey": null,
                "elementIndex": null
            }]
        })
    );

    let invalid_upsert = proxy.process_request(json_graphql_request(
        upsert_query,
        json!({
            "handle": {"type": "handle_validation_type", "handle": "hello world!"},
            "metaobject": {"fields": [{"key": "title", "value": "Handle hello world!"}]}
        }),
    ));
    assert_eq!(
        invalid_upsert.body["data"]["metaobjectUpsert"],
        json!({
            "metaobject": null,
            "userErrors": [{
                "field": ["handle", "handle"],
                "message": "Handle is invalid",
                "code": "INVALID",
                "elementKey": null,
                "elementIndex": null
            }]
        })
    );

    let too_long = "x".repeat(256);
    let too_long_upsert = proxy.process_request(json_graphql_request(
        upsert_query,
        json!({
            "handle": {"type": "handle_validation_type", "handle": too_long},
            "metaobject": {"fields": [{"key": "title", "value": "Handle too long"}]}
        }),
    ));
    assert_eq!(
        too_long_upsert.body["data"]["metaobjectUpsert"],
        json!({
            "metaobject": null,
            "userErrors": [{
                "field": ["handle", "handle"],
                "message": "Handle is too long (maximum is 255 characters)",
                "code": "TOO_LONG",
                "elementKey": null,
                "elementIndex": null
            }]
        })
    );

    let after_rejects = proxy.process_request(json_graphql_request(
        read_query,
        json!({
            "type": "handle_validation_type",
            "handle": {"type": "handle_validation_type", "handle": "hello-world"}
        }),
    ));
    assert_eq!(
        after_rejects.body["data"]["definition"]["metaobjectsCount"],
        json!(0)
    );
    assert_eq!(after_rejects.body["data"]["entries"]["nodes"], json!([]));
    assert_eq!(after_rejects.body["data"]["byHandle"], Value::Null);

    let blank_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({"metaobject": {
            "type": "handle_validation_type",
            "handle": "",
            "fields": [{"key": "title", "value": "Blank create"}]
        }}),
    ));
    assert_eq!(
        blank_create.body["data"]["metaobjectCreate"]["userErrors"],
        json!([])
    );
    let blank_create_metaobject = &blank_create.body["data"]["metaobjectCreate"]["metaobject"];
    let blank_create_id = blank_create_metaobject["id"].as_str().unwrap().to_string();
    let blank_create_handle = blank_create_metaobject["handle"].as_str().unwrap();
    assert_eq!(blank_create_handle, "blank-create");
    assert_eq!(
        blank_create_metaobject["displayName"],
        json!("Blank create")
    );

    let blank_upsert = proxy.process_request(json_graphql_request(
        upsert_query,
        json!({
            "handle": {"type": "handle_validation_type", "handle": ""},
            "metaobject": {"fields": [{"key": "title", "value": "Blank upsert"}]}
        }),
    ));
    assert_eq!(
        blank_upsert.body["data"]["metaobjectUpsert"]["userErrors"],
        json!([])
    );
    let blank_upsert_metaobject = &blank_upsert.body["data"]["metaobjectUpsert"]["metaobject"];
    let blank_upsert_handle = blank_upsert_metaobject["handle"].as_str().unwrap();
    assert_eq!(blank_upsert_handle, "blank-upsert");
    assert_eq!(
        blank_upsert_metaobject["displayName"],
        json!("Blank upsert")
    );

    let update_invalid = proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": blank_create_id, "metaobject": {
            "handle": "hello world!",
            "fields": [{"key": "title", "value": "Update invalid"}]
        }}),
    ));
    assert_eq!(
        update_invalid.body["data"]["metaobjectUpdate"],
        json!({
            "metaobject": null,
            "userErrors": [{
                "field": ["metaobject", "handle"],
                "message": "Handle is invalid",
                "code": "INVALID",
                "elementKey": null,
                "elementIndex": null
            }]
        })
    );
}

#[test]
fn metaobject_create_validates_definition_fields_and_capabilities() {
    let mut proxy = snapshot_proxy();

    proxy.process_request(json_graphql_request(
        r#"
        mutation CreateValidationDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"definition": {
            "type": "validation_metaobject_type",
            "name": "Validation Metaobject",
            "displayNameKey": "title",
            "fieldDefinitions": [
                {"key": "title", "name": "Title", "type": "single_line_text_field", "required": true},
                {"key": "quantity", "name": "Quantity", "type": "number_integer", "required": false}
            ]
        }}),
    ));

    let unknown_type = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMissingType($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"metaobject": {"type": "missing_metaobject_type", "fields": [{"key": "title", "value": "Missing"}]}}),
    ));
    assert_eq!(
        unknown_type.body["data"]["metaobjectCreate"]["userErrors"][0]["code"],
        json!("UNDEFINED_OBJECT_TYPE")
    );

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateInvalidMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"metaobject": {
            "type": "validation_metaobject_type",
            "capabilities": {"publishable": {"status": "ACTIVE"}},
            "fields": [
                {"key": "quantity", "value": "not-an-int"},
                {"key": "quantity", "value": "2"},
                {"key": "unknown", "value": "ignored"}
            ]
        }}),
    ));
    let codes = invalid.body["data"]["metaobjectCreate"]["userErrors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|error| error["code"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(codes.contains(&"OBJECT_FIELD_REQUIRED"));
    assert!(codes.contains(&"DUPLICATE_FIELD_INPUT"));
    assert!(codes.contains(&"UNDEFINED_OBJECT_FIELD"));
    assert!(codes.contains(&"INVALID_VALUE"));
    assert!(codes.contains(&"CAPABILITY_NOT_ENABLED"));
    let capability_error = invalid.body["data"]["metaobjectCreate"]["userErrors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|error| error["code"] == "CAPABILITY_NOT_ENABLED")
        .unwrap();
    assert_eq!(
        capability_error["field"],
        json!(["metaobject", "capabilities", "publishable"])
    );
    assert_eq!(
        capability_error["message"],
        json!("Capability is not enabled: publishable")
    );
    assert_eq!(
        invalid.body["data"]["metaobjectCreate"]["metaobject"],
        Value::Null
    );
}

#[test]
fn metaobject_create_update_and_upsert_reject_extended_field_value_types() {
    let mut proxy = snapshot_proxy();
    let title_field = json!({"key": "title", "name": "Title", "type": "single_line_text_field", "required": false});
    let target_definition_id = create_metaobject_definition_for_test(
        &mut proxy,
        "field_validation_target_type",
        vec![title_field.clone()],
    );
    create_metaobject_definition_for_test(
        &mut proxy,
        "field_validation_matrix_type",
        vec![
            title_field,
            json!({"key": "money", "name": "Money", "type": "money", "required": false}),
            json!({"key": "link", "name": "Link", "type": "link", "required": false}),
            json!({"key": "link_domain", "name": "Link Domain", "type": "link", "required": false, "validations": [{"name": "allowed_domains", "value": "[\"example.com\"]"}]}),
            json!({"key": "language", "name": "Language", "type": "language", "required": false}),
            json!({"key": "power", "name": "Power", "type": "power", "required": false}),
            json!({"key": "file_reference", "name": "File", "type": "file_reference", "required": false}),
            json!({"key": "page_reference", "name": "Page", "type": "page_reference", "required": false}),
            json!({"key": "order_reference", "name": "Order", "type": "order_reference", "required": false}),
            json!({"key": "article_reference", "name": "Article", "type": "article_reference", "required": false}),
            json!({"key": "product_taxonomy_value_reference", "name": "Product Taxonomy Value", "type": "product_taxonomy_value_reference", "required": false, "validations": [{"name": "product_taxonomy_attribute_handle", "value": "material"}]}),
            json!({"key": "mixed_reference", "name": "Mixed", "type": "mixed_reference", "required": false, "validations": [{"name": "metaobject_definition_ids", "value": json!([target_definition_id]).to_string()}]}),
        ],
    );

    let create_query = r#"
        mutation CreateMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let update_query = r#"
        mutation UpdateMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let upsert_query = r#"
        mutation UpsertMetaobject($handle: MetaobjectHandleInput!, $metaobject: MetaobjectUpsertInput!) {
          metaobjectUpsert(handle: $handle, metaobject: $metaobject) {
            metaobject { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;

    let setup = proxy.process_request(json_graphql_request(
        create_query,
        json!({"metaobject": {
            "type": "field_validation_matrix_type",
            "handle": "field-validation-setup",
            "fields": [{"key": "title", "value": "Validation setup"}]
        }}),
    ));
    assert_eq!(
        setup.body["data"]["metaobjectCreate"]["userErrors"],
        json!([])
    );
    let setup_id = setup.body["data"]["metaobjectCreate"]["metaobject"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let cases = [
        (
            "money",
            "not-money".to_string(),
            "Value must be a stringified JSON object with amount (numeric) and currency_code (string matching the shop's currency) fields.",
        ),
        (
            "link",
            json!({"text": "Docs", "url": "ftp://nope"}).to_string(),
            "Value must be one of the following URL schemes: http, https, mailto, sms, tel.",
        ),
        (
            "link_domain",
            json!({"text": "Docs", "url": "https://not-example.com/path"}).to_string(),
            "Value must conform to the domain restriction you set.",
        ),
        (
            "language",
            "not-a-language".to_string(),
            "Value must be in ISO 639-1 format.",
        ),
        (
            "power",
            "10".to_string(),
            "Value must be a stringified JSON object with a value (numeric) and unit (string from one the supported measurement units) fields.",
        ),
        (
            "file_reference",
            "gid://shopify/Product/1".to_string(),
            "Value must be a file reference string.",
        ),
        (
            "page_reference",
            "gid://shopify/Product/1".to_string(),
            "Value must be a valid page reference.",
        ),
        (
            "order_reference",
            "gid://shopify/Product/1".to_string(),
            "Value must be a valid order reference.",
        ),
        (
            "article_reference",
            "gid://shopify/Product/1".to_string(),
            "Value must be a valid article reference.",
        ),
        (
            "product_taxonomy_value_reference",
            "gid://shopify/Product/1".to_string(),
            "Value require that you select a product taxonomy value.",
        ),
        (
            "mixed_reference",
            "gid://shopify/Product/1".to_string(),
            "Value must belong to one of the specified metaobject definitions.",
        ),
    ];

    for (index, (key, value, message)) in cases.iter().enumerate() {
        let expected_error = json!([{
            "field": ["metaobject", "fields", "0"],
            "message": message,
            "code": "INVALID_VALUE",
            "elementKey": key,
            "elementIndex": null
        }]);
        let handle = format!("field-validation-create-{index}");
        let create = proxy.process_request(json_graphql_request(
            create_query,
            json!({"metaobject": {
                "type": "field_validation_matrix_type",
                "handle": handle,
                "fields": [{"key": key, "value": value}]
            }}),
        ));
        assert_eq!(
            create.body["data"]["metaobjectCreate"]["metaobject"],
            Value::Null
        );
        assert_eq!(
            create.body["data"]["metaobjectCreate"]["userErrors"],
            expected_error
        );

        let update = proxy.process_request(json_graphql_request(
            update_query,
            json!({"id": setup_id, "metaobject": {"fields": [{"key": key, "value": value}]}}),
        ));
        assert_eq!(
            update.body["data"]["metaobjectUpdate"]["metaobject"],
            Value::Null
        );
        assert_eq!(
            update.body["data"]["metaobjectUpdate"]["userErrors"],
            expected_error
        );

        let upsert_create = proxy.process_request(json_graphql_request(
            upsert_query,
            json!({
                "handle": {"type": "field_validation_matrix_type", "handle": format!("field-validation-upsert-{index}")},
                "metaobject": {"fields": [{"key": key, "value": value}]}
            }),
        ));
        assert_eq!(
            upsert_create.body["data"]["metaobjectUpsert"]["metaobject"],
            Value::Null
        );
        assert_eq!(
            upsert_create.body["data"]["metaobjectUpsert"]["userErrors"],
            expected_error
        );
    }
}

#[test]
fn metaobject_mixed_reference_accepts_unobserved_metaobject_gid_when_definition_limited() {
    let mut proxy = snapshot_proxy();
    let title_field = json!({"key": "title", "name": "Title", "type": "single_line_text_field", "required": false});
    let target_definition_id = create_metaobject_definition_for_test(
        &mut proxy,
        "mixed_reference_target_type",
        vec![title_field.clone()],
    );
    create_metaobject_definition_for_test(
        &mut proxy,
        "mixed_reference_matrix_type",
        vec![
            title_field,
            json!({"key": "mixed_reference", "name": "Mixed", "type": "mixed_reference", "required": false, "validations": [{"name": "metaobject_definition_ids", "value": json!([target_definition_id]).to_string()}]}),
        ],
    );

    let remote_metaobject_id = "gid://shopify/Metaobject/185981075762";
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateMixedReferenceMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { field(key: "mixed_reference") { key value } }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"metaobject": {
            "type": "mixed_reference_matrix_type",
            "handle": "remote-mixed-reference",
            "fields": [
                {"key": "title", "value": "Remote mixed reference"},
                {"key": "mixed_reference", "value": remote_metaobject_id}
            ]
        }}),
    ));

    assert_eq!(
        create.body["data"]["metaobjectCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["metaobjectCreate"]["metaobject"]["field"],
        json!({"key": "mixed_reference", "value": remote_metaobject_id})
    );
}

#[test]
fn metaobject_update_reports_undefined_input_and_new_required_schema_field() {
    let mut proxy = snapshot_proxy();
    let definition_id = create_metaobject_definition_for_test(
        &mut proxy,
        "schema_change_required_type",
        vec![
            json!({"key": "title", "name": "Title", "type": "single_line_text_field", "required": true}),
            json!({"key": "legacy", "name": "Legacy", "type": "single_line_text_field", "required": false}),
        ],
    );
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateSchemaChangeMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"metaobject": {
            "type": "schema_change_required_type",
            "handle": "schema-change-required",
            "fields": [
                {"key": "title", "value": "Schema change row"},
                {"key": "legacy", "value": "Legacy value"}
            ]
        }}),
    ));
    assert_eq!(
        create.body["data"]["metaobjectCreate"]["userErrors"],
        json!([])
    );
    let metaobject_id = create.body["data"]["metaobjectCreate"]["metaobject"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let definition_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateSchemaDefinition($id: ID!, $definition: MetaobjectDefinitionUpdateInput!) {
          metaobjectDefinitionUpdate(id: $id, definition: $definition) {
            metaobjectDefinition { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"id": definition_id, "definition": {
            "fieldDefinitions": [
                {"delete": {"key": "legacy"}},
                {"create": {"key": "summary", "name": "Summary", "type": "single_line_text_field", "required": true}}
            ]
        }}),
    ));
    assert_eq!(
        definition_update.body["data"]["metaobjectDefinitionUpdate"]["userErrors"],
        json!([])
    );

    let stale_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateSchemaChangeMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"id": metaobject_id, "metaobject": {
            "fields": [{"key": "legacy", "value": "Still stale"}]
        }}),
    ));

    assert_eq!(
        stale_update.body["data"]["metaobjectUpdate"]["metaobject"],
        Value::Null
    );
    assert_eq!(
        stale_update.body["data"]["metaobjectUpdate"]["userErrors"],
        json!([
            {
                "field": ["metaobject", "fields", "0"],
                "message": "Field definition \"legacy\" does not exist",
                "code": "UNDEFINED_OBJECT_FIELD",
                "elementKey": "legacy",
                "elementIndex": null
            },
            {
                "field": ["metaobject"],
                "message": "Summary can't be blank",
                "code": "OBJECT_FIELD_REQUIRED",
                "elementKey": "summary",
                "elementIndex": null
            }
        ])
    );
}

#[test]
fn metaobject_id_values_are_unique_and_merged_update_values_are_revalidated() {
    let mut proxy = snapshot_proxy();
    let title_field = json!({"key": "title", "name": "Title", "type": "single_line_text_field", "required": false});
    create_metaobject_definition_for_test(
        &mut proxy,
        "id_validation_type",
        vec![
            title_field,
            json!({"key": "custom_id", "name": "Custom ID", "type": "id", "required": false}),
        ],
    );

    let create_query = r#"
        mutation CreateMetaobject($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) {
            metaobject { id handle }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let upsert_query = r#"
        mutation UpsertMetaobject($handle: MetaobjectHandleInput!, $metaobject: MetaobjectUpsertInput!) {
          metaobjectUpsert(handle: $handle, metaobject: $metaobject) {
            metaobject { id handle }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let update_query = r#"
        mutation UpdateMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject { id handle }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#;
    let first = proxy.process_request(json_graphql_request(
        create_query,
        json!({"metaobject": {
            "type": "id_validation_type",
            "handle": "id-validation-first",
            "fields": [{"key": "custom_id", "value": "shared-id"}]
        }}),
    ));
    assert_eq!(
        first.body["data"]["metaobjectCreate"]["userErrors"],
        json!([])
    );
    let second = proxy.process_request(json_graphql_request(
        create_query,
        json!({"metaobject": {
            "type": "id_validation_type",
            "handle": "id-validation-second",
            "fields": [{"key": "title", "value": "Second"}]
        }}),
    ));
    let second_id = second.body["data"]["metaobjectCreate"]["metaobject"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let expected_taken = json!([{
        "field": ["metaobject", "fields", "0"],
        "message": "Value is already assigned to another metafield. Choose a different value to ensure it remains unique.",
        "code": "TAKEN",
        "elementKey": "custom_id",
        "elementIndex": null
    }]);
    let duplicate_create = proxy.process_request(json_graphql_request(
        create_query,
        json!({"metaobject": {
            "type": "id_validation_type",
            "handle": "id-validation-duplicate",
            "fields": [{"key": "custom_id", "value": "shared-id"}]
        }}),
    ));
    assert_eq!(
        duplicate_create.body["data"]["metaobjectCreate"]["userErrors"],
        expected_taken
    );

    let duplicate_update = proxy.process_request(json_graphql_request(
        update_query,
        json!({"id": second_id, "metaobject": {"fields": [{"key": "custom_id", "value": "shared-id"}]}}),
    ));
    assert_eq!(
        duplicate_update.body["data"]["metaobjectUpdate"]["userErrors"],
        expected_taken
    );

    let duplicate_upsert = proxy.process_request(json_graphql_request(
        upsert_query,
        json!({
            "handle": {"type": "id_validation_type", "handle": "id-validation-second"},
            "metaobject": {"fields": [{"key": "custom_id", "value": "shared-id"}]}
        }),
    ));
    assert_eq!(
        duplicate_upsert.body["data"]["metaobjectUpsert"]["userErrors"],
        expected_taken
    );

    let stale_definition_id = create_metaobject_definition_for_test(
        &mut proxy,
        "stale_revalidation_type",
        vec![
            json!({"key": "title", "name": "Title", "type": "single_line_text_field", "required": false}),
            json!({"key": "body", "name": "Body", "type": "single_line_text_field", "required": false}),
        ],
    );
    let stale = proxy.process_request(json_graphql_request(
        create_query,
        json!({"metaobject": {
            "type": "stale_revalidation_type",
            "handle": "stale-revalidation",
            "fields": [
                {"key": "title", "value": "Stale row"},
                {"key": "body", "value": "abcdef"}
            ]
        }}),
    ));
    let stale_id = stale.body["data"]["metaobjectCreate"]["metaobject"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let update_definition = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateDefinition($id: ID!, $definition: MetaobjectDefinitionUpdateInput!) {
          metaobjectDefinitionUpdate(id: $id, definition: $definition) {
            metaobjectDefinition { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"id": stale_definition_id, "definition": {
            "fieldDefinitions": [{"update": {"key": "body", "validations": [{"name": "max", "value": "3"}]}}]
        }}),
    ));
    assert_eq!(
        update_definition.body["data"]["metaobjectDefinitionUpdate"]["userErrors"],
        json!([])
    );
    let stale_update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateMetaobject($id: ID!, $metaobject: MetaobjectUpdateInput!) {
          metaobjectUpdate(id: $id, metaobject: $metaobject) {
            metaobject { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"id": stale_id, "metaobject": {"fields": [{"key": "title", "value": "Still stale"}]}}),
    ));
    assert_eq!(
        stale_update.body["data"]["metaobjectUpdate"]["userErrors"],
        json!([{
            "field": ["metaobject"],
            "message": "Value has a maximum length of 3.",
            "code": "INVALID_VALUE",
            "elementKey": "body",
            "elementIndex": null
        }])
    );
}

#[test]
fn metaobject_delete_returns_record_not_found_without_logging_noop_deletes() {
    let mut proxy = snapshot_proxy();

    let delete_query = r#"
        mutation ArbitraryMetaobjectDelete($id: ID!) {
          metaobjectDelete(id: $id) { deletedId userErrors { field message code elementKey elementIndex } }
        }
        "#;
    let record_not_found = json!({
        "deletedId": null,
        "userErrors": [{
            "field": ["id"],
            "message": "Record not found",
            "code": "RECORD_NOT_FOUND",
            "elementKey": null,
            "elementIndex": null
        }]
    });

    let unknown = proxy.process_request(json_graphql_request(
        delete_query,
        json!({"id": "gid://shopify/Metaobject/does-not-exist"}),
    ));
    assert_eq!(unknown.body["data"]["metaobjectDelete"], record_not_found);

    let malformed = proxy.process_request(json_graphql_request(
        delete_query,
        json!({"id": "not-a-shopify-gid"}),
    ));
    assert_eq!(malformed.body["data"]["metaobjectDelete"], record_not_found);

    assert_eq!(
        proxy
            .process_request(Request {
                method: "GET".to_string(),
                path: "/__meta/log".to_string(),
                ..Default::default()
            })
            .body,
        json!({"entries": []})
    );

    let definition = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDeleteTargetDefinition($definition: MetaobjectDefinitionCreateInput!) {
          metaobjectDefinitionCreate(definition: $definition) {
            metaobjectDefinition { id }
            userErrors { field message code elementKey elementIndex }
          }
        }
        "#,
        json!({"definition": {
            "type": "delete_test",
            "name": "Delete Test",
            "displayNameKey": "title",
            "fieldDefinitions": [
                {"key": "title", "name": "Title", "type": "single_line_text_field", "required": true}
            ]
        }}),
    ));
    assert_eq!(
        definition.body["data"]["metaobjectDefinitionCreate"]["userErrors"],
        json!([])
    );

    let created = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateDeleteTarget($metaobject: MetaobjectCreateInput!) {
          metaobjectCreate(metaobject: $metaobject) { metaobject { id } userErrors { field message code } }
        }
        "#,
        json!({"metaobject": {"type": "delete_test", "handle": "delete-test", "fields": [{"key": "title", "value": "Delete test"}]}}),
    ));
    let staged_id = created.body["data"]["metaobjectCreate"]["metaobject"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let deleted =
        proxy.process_request(json_graphql_request(delete_query, json!({"id": staged_id})));
    assert_eq!(
        deleted.body["data"]["metaobjectDelete"],
        json!({"deletedId": created.body["data"]["metaobjectCreate"]["metaobject"]["id"], "userErrors": []})
    );

    let repeated =
        proxy.process_request(json_graphql_request(delete_query, json!({"id": staged_id})));
    assert_eq!(repeated.body["data"]["metaobjectDelete"], record_not_found);

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        ..Default::default()
    });
    assert_eq!(log.body["entries"].as_array().unwrap().len(), 3);
    assert_eq!(
        log.body["entries"][2]["stagedResourceIds"],
        json!([created.body["data"]["metaobjectCreate"]["metaobject"]["id"]])
    );
}

#[test]
fn media_file_lifecycle_stages_uploaded_reads_and_empty_product_media_after_delete() {
    // Seed the products the file flow references so their (empty) media
    // connection is COMPUTED from store state rather than replayed from a
    // captured upstream read. The proxy never fabricates products it never
    // saw, so without these seeds `product(...)` correctly returns null.
    let mut proxy = snapshot_proxy().with_base_products(vec![
        ProductRecord {
            id: "gid://shopify/Product/429001".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "File reference target".to_string(),
            handle: "file-reference-target".to_string(),
            status: "ACTIVE".to_string(),
            ..ProductRecord::default()
        },
        ProductRecord {
            id: "gid://shopify/Product/9264121479401".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            updated_at: "2024-01-01T00:00:00.000Z".to_string(),
            title: "Reference cleanup target".to_string(),
            handle: "reference-cleanup-target".to_string(),
            status: "ACTIVE".to_string(),
            ..ProductRecord::default()
        },
    ]);

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation FileReferenceCreate($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id alt createdAt fileStatus filename ... on MediaImage { image { url width height } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [{"alt": "Reference source", "contentType": "IMAGE", "filename": "reference-source.jpg", "originalSource": "https://cdn.example.com/reference-source.jpg"}]}),
    ));
    assert_eq!(
        create.body["data"]["fileCreate"],
        json!({
            "files": [{"id": "gid://shopify/MediaImage/2", "alt": "Reference source", "createdAt": "2024-01-01T00:00:01.000Z", "fileStatus": "UPLOADED", "filename": "reference-source.jpg", "image": {"url": "https://cdn.example.com/reference-source.jpg", "width": null, "height": null}}],
            "userErrors": []
        })
    );

    let attach = proxy.process_request(json_graphql_request(
        r#"
        mutation FileReferenceAttach($files: [FileUpdateInput!]!) {
          fileUpdate(files: $files) { files { id alt fileStatus ... on MediaImage { image { url } } } userErrors { field message code } }
        }
        "#,
        json!({"files": [{"id": "gid://shopify/MediaImage/2", "alt": "Attached file media", "originalSource": "https://cdn.example.com/file-reference-ready.jpg", "referencesToAdd": ["gid://shopify/Product/429001"]}]}),
    ));
    assert_eq!(
        attach.body["data"]["fileUpdate"],
        json!({"files": [], "userErrors": [{"field": ["files"], "message": "Non-ready files cannot be updated.", "code": "NON_READY_STATE"}]})
    );

    let product_read = proxy.process_request(json_graphql_request(
        r#"
        query FileReferenceProductRead($productId: ID!) {
          product(id: $productId) { id title media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } } }
        }
        "#,
        json!({"productId": "gid://shopify/Product/429001"}),
    ));
    assert_eq!(
        product_read.body["data"]["product"],
        json!({"id": "gid://shopify/Product/429001", "title": "File reference target", "media": {"nodes": [], "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": null, "endCursor": null}}})
    );

    let files_read = proxy.process_request(json_graphql_request(
        r#"
        query FileReferenceFilesRead {
          files(first: 10) { nodes { id alt createdAt fileStatus filename ... on MediaImage { image { url width height } } } pageInfo { hasNextPage hasPreviousPage startCursor endCursor } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        files_read.body["data"]["files"],
        json!({"nodes": [{"id": "gid://shopify/MediaImage/2", "alt": "Reference source", "createdAt": "2024-01-01T00:00:01.000Z", "fileStatus": "READY", "filename": "reference-source.jpg", "image": {"url": "https://cdn.example.com/reference-source.jpg", "width": null, "height": null}}], "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": "cursor:gid://shopify/MediaImage/2", "endCursor": "cursor:gid://shopify/MediaImage/2"}})
    );

    // Delete the file this test actually created (MediaImage/2). The engine
    // computes deletion against real store state, so deleting a captured id it
    // never saw would (correctly) return FILE_DOES_NOT_EXIST — exercise the real
    // create→read→delete lifecycle instead of replaying a phantom id.
    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation FileDeleteParity($fileIds: [ID!]!) {
          fileDelete(fileIds: $fileIds) { deletedFileIds userErrors { field message code } }
        }
        "#,
        json!({"fileIds": ["gid://shopify/MediaImage/2"]}),
    ));
    assert_eq!(
        delete.body["data"]["fileDelete"],
        json!({"deletedFileIds": ["gid://shopify/MediaImage/2"], "userErrors": []})
    );

    let post_delete = proxy.process_request(json_graphql_request(
        r#"
        query FileDeleteMediaReferenceDownstream($id: ID!) {
          product(id: $id) { id media(first: 10) { nodes { id alt mediaContentType status preview { image { url } } ... on MediaImage { image { url } } } } }
        }
        "#,
        json!({"id": "gid://shopify/Product/9264121479401"}),
    ));
    assert_eq!(
        post_delete.body["data"]["product"],
        json!({"id": "gid://shopify/Product/9264121479401", "media": {"nodes": []}})
    );
}

#[test]
fn media_files_read_returns_staged_files_and_empty_file_saved_searches() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation FilesUploadRuntimeCoverageCreate($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id alt createdAt fileStatus filename ... on MediaImage { image { url width height } preview { image { url } } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [{"alt": "Local runtime file", "contentType": "IMAGE", "filename": "local-runtime.jpg", "originalSource": "https://cdn.example.com/local-runtime.jpg"}]}),
    ));
    assert_eq!(
        create.body["data"]["fileCreate"],
        json!({
            "files": [{
                "id": "gid://shopify/MediaImage/2",
                "alt": "Local runtime file",
                "createdAt": "2024-01-01T00:00:01.000Z",
                "fileStatus": "UPLOADED",
                "filename": "local-runtime.jpg",
                "image": {
                    "url": "https://cdn.example.com/local-runtime.jpg",
                    "width": null,
                    "height": null
                },
                "preview": {
                    "image": {
                        "url": "https://cdn.example.com/local-runtime.jpg"
                    }
                }
            }],
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query FilesUploadRuntimeCoverageRead {
          files(first: 10) {
            nodes { id alt createdAt fileStatus filename ... on MediaImage { image { url width height } preview { image { url } } } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          fileSavedSearches(first: 5) {
            nodes { id name }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"],
        json!({
            "files": {
                "nodes": [{
                    "id": "gid://shopify/MediaImage/2",
                    "alt": "Local runtime file",
                    "createdAt": "2024-01-01T00:00:01.000Z",
                    "fileStatus": "READY",
                    "filename": "local-runtime.jpg",
                    "image": {
                        "url": "https://cdn.example.com/local-runtime.jpg",
                        "width": null,
                        "height": null
                    },
                    "preview": {
                        "image": {
                            "url": "https://cdn.example.com/local-runtime.jpg"
                        }
                    }
                }],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": "cursor:gid://shopify/MediaImage/2",
                    "endCursor": "cursor:gid://shopify/MediaImage/2"
                }
            },
            "fileSavedSearches": {
                "nodes": [],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": null,
                    "endCursor": null
                }
            }
        })
    );
}

#[test]
fn media_file_create_poll_ready_then_update_succeeds() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaFileReadyLifecycleCreate($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id alt createdAt fileStatus filename }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [{"alt": "Ready lifecycle", "contentType": "IMAGE", "filename": "ready-lifecycle.jpg", "originalSource": "https://cdn.example.com/ready-lifecycle.jpg"}]}),
    ));
    assert_eq!(create.body["data"]["fileCreate"]["userErrors"], json!([]));
    let file_id = create.body["data"]["fileCreate"]["files"][0]["id"]
        .as_str()
        .expect("fileCreate should return an id")
        .to_string();
    assert_eq!(
        create.body["data"]["fileCreate"]["files"][0]["fileStatus"],
        json!("UPLOADED")
    );

    let poll = proxy.process_request(json_graphql_request(
        r#"
        query MediaFileReadyLifecyclePoll {
          files(first: 5) {
            nodes { id alt fileStatus filename }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        poll.body["data"]["files"],
        json!({
            "nodes": [{
                "id": file_id,
                "alt": "Ready lifecycle",
                "fileStatus": "READY",
                "filename": "ready-lifecycle.jpg"
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": format!("cursor:{file_id}"),
                "endCursor": format!("cursor:{file_id}")
            }
        })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaFileReadyLifecycleUpdate($files: [FileUpdateInput!]!) {
          fileUpdate(files: $files) {
            files { id alt fileStatus filename }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [{"id": file_id, "alt": "Updated after ready"}]}),
    ));
    assert_eq!(
        update.body["data"]["fileUpdate"],
        json!({
            "files": [{
                "id": file_id,
                "alt": "Updated after ready",
                "fileStatus": "READY",
                "filename": "ready-lifecycle.jpg"
            }],
            "userErrors": []
        })
    );
}

#[test]
fn media_files_live_hybrid_cold_read_forwards_and_hydrates_files_and_saved_searches() {
    let upstream_bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_bodies = Arc::clone(&upstream_bodies);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            captured_bodies.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "files": {
                            "nodes": [{
                                "__typename": "MediaImage",
                                "id": "gid://shopify/MediaImage/777",
                                "alt": "Upstream file",
                                "createdAt": "2026-07-01T00:00:00Z",
                                "updatedAt": "2026-07-01T00:00:00Z",
                                "fileStatus": "READY",
                                "filename": "upstream-file.jpg",
                                "image": {
                                    "url": "https://cdn.example.com/upstream-file.jpg",
                                    "width": 1200,
                                    "height": 800
                                }
                            }, {
                                "__typename": "MediaImage",
                                "id": "gid://shopify/MediaImage/778",
                                "alt": "Upstream processing file",
                                "createdAt": "2026-07-01T00:00:01Z",
                                "updatedAt": "2026-07-01T00:00:01Z",
                                "fileStatus": "PROCESSING",
                                "filename": "upstream-processing-file.jpg",
                                "image": {
                                    "url": "https://cdn.example.com/upstream-processing-file.jpg",
                                    "width": 640,
                                    "height": 480
                                }
                            }],
                            "pageInfo": {
                                "hasNextPage": false,
                                "hasPreviousPage": false,
                                "startCursor": "cursor:gid://shopify/MediaImage/777",
                                "endCursor": "cursor:gid://shopify/MediaImage/778"
                            }
                        },
                        "fileSavedSearches": {
                            "nodes": [{
                                "id": "gid://shopify/SavedSearch/888",
                                "name": "Ready files",
                                "query": "file_status:ready",
                                "resourceType": "FILE"
                            }],
                            "pageInfo": {
                                "hasNextPage": false,
                                "hasPreviousPage": false,
                                "startCursor": "cursor:gid://shopify/SavedSearch/888",
                                "endCursor": "cursor:gid://shopify/SavedSearch/888"
                            }
                        }
                    }
                }),
            }
        });

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MediaFilesColdRead {
          files(first: 5) {
            nodes {
              id
              alt
              fileStatus
              filename
              ... on MediaImage { image { url width height } }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          fileSavedSearches(first: 5) {
            nodes { id name query resourceType }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        read.body["data"]["files"],
        json!({
            "nodes": [{
                "id": "gid://shopify/MediaImage/777",
                "alt": "Upstream file",
                "fileStatus": "READY",
                "filename": "upstream-file.jpg",
                "image": {
                    "url": "https://cdn.example.com/upstream-file.jpg",
                    "width": 1200,
                    "height": 800
                }
            }, {
                "id": "gid://shopify/MediaImage/778",
                "alt": "Upstream processing file",
                "fileStatus": "PROCESSING",
                "filename": "upstream-processing-file.jpg",
                "image": {
                    "url": "https://cdn.example.com/upstream-processing-file.jpg",
                    "width": 640,
                    "height": 480
                }
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "cursor:gid://shopify/MediaImage/777",
                "endCursor": "cursor:gid://shopify/MediaImage/778"
            }
        })
    );
    assert_eq!(
        read.body["data"]["fileSavedSearches"],
        json!({
            "nodes": [{
                "id": "gid://shopify/SavedSearch/888",
                "name": "Ready files",
                "query": "file_status:ready",
                "resourceType": "FILE"
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "cursor:gid://shopify/SavedSearch/888",
                "endCursor": "cursor:gid://shopify/SavedSearch/888"
            }
        })
    );
    let bodies = upstream_bodies.lock().unwrap();
    assert_eq!(
        bodies.len(),
        1,
        "cold media files read should forward upstream"
    );
    assert!(bodies[0]["query"]
        .as_str()
        .is_some_and(|query| query.contains("files") && query.contains("fileSavedSearches")));
}

#[test]
fn media_file_saved_searches_live_hybrid_cold_read_forwards_standalone_root() {
    let upstream_bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_bodies = Arc::clone(&upstream_bodies);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            captured_bodies.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "fileSavedSearches": {
                            "nodes": [{
                                "id": "gid://shopify/SavedSearch/889",
                                "name": "Recent files",
                                "query": "created_at:>=2026-01-01",
                                "resourceType": "FILE"
                            }],
                            "pageInfo": {
                                "hasNextPage": false,
                                "hasPreviousPage": false,
                                "startCursor": "cursor:gid://shopify/SavedSearch/889",
                                "endCursor": "cursor:gid://shopify/SavedSearch/889"
                            }
                        }
                    }
                }),
            }
        });

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MediaFileSavedSearchColdRead {
          fileSavedSearches(first: 5) {
            nodes { id name query resourceType }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        read.body["data"]["fileSavedSearches"],
        json!({
            "nodes": [{
                "id": "gid://shopify/SavedSearch/889",
                "name": "Recent files",
                "query": "created_at:>=2026-01-01",
                "resourceType": "FILE"
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "cursor:gid://shopify/SavedSearch/889",
                "endCursor": "cursor:gid://shopify/SavedSearch/889"
            }
        })
    );
    assert_eq!(
        upstream_bodies.lock().unwrap().len(),
        1,
        "standalone fileSavedSearches read should forward upstream"
    );
}

#[test]
fn media_files_query_filters_and_sort_keys_apply_to_staged_files() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation FileQuerySortCreate($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id filename contentType createdAt updatedAt fileStatus }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [
            {"alt": "Zulu image", "contentType": "IMAGE", "filename": "zulu-image.jpg", "originalSource": "https://cdn.example.com/zulu-image.jpg"},
            {"alt": "Alpha file", "contentType": "FILE", "filename": "alpha-file.pdf", "originalSource": "https://cdn.example.com/alpha-file.pdf"},
            {"alt": "Middle image", "contentType": "IMAGE", "filename": "middle-image.jpg", "originalSource": "https://cdn.example.com/middle-image.jpg"}
        ]}),
    ));
    assert_eq!(create.body["data"]["fileCreate"]["userErrors"], json!([]));

    let read = proxy.process_request(json_graphql_request(
        r#"
        query FileQuerySortRead {
          filename: files(first: 10, query: "filename:alpha-file.pdf") {
            nodes { id filename contentType fileStatus }
          }
          mediaType: files(first: 10, query: "media_type:IMAGE") {
            nodes { filename contentType }
          }
          unknown: files(first: 10, query: "definitely_not_a_file_filter:value") {
            nodes { filename }
          }
          byFilename: files(first: 10, sortKey: FILENAME) {
            nodes { filename }
          }
          byFilenameReverse: files(first: 10, sortKey: FILENAME, reverse: true) {
            nodes { filename }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        read.body["data"]["filename"]["nodes"],
        json!([{
            "id": "gid://shopify/GenericFile/3",
            "filename": "alpha-file.pdf",
            "contentType": "FILE",
            "fileStatus": "READY"
        }])
    );
    assert_eq!(
        read.body["data"]["mediaType"]["nodes"],
        json!([
            {"filename": "zulu-image.jpg", "contentType": "IMAGE"},
            {"filename": "middle-image.jpg", "contentType": "IMAGE"}
        ])
    );
    assert_eq!(read.body["data"]["unknown"]["nodes"], json!([]));
    assert_eq!(
        read.body["data"]["byFilename"]["nodes"],
        json!([
            {"filename": "alpha-file.pdf"},
            {"filename": "middle-image.jpg"},
            {"filename": "zulu-image.jpg"}
        ])
    );
    assert_eq!(
        read.body["data"]["byFilenameReverse"]["nodes"],
        json!([
            {"filename": "zulu-image.jpg"},
            {"filename": "middle-image.jpg"},
            {"filename": "alpha-file.pdf"}
        ])
    );
}

#[test]
fn media_files_saved_search_read_and_saved_search_id_filter_use_staged_records() {
    let mut proxy = snapshot_proxy();

    let create_files = proxy.process_request(json_graphql_request(
        r#"
        mutation FileSavedSearchCreateFiles($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id filename contentType }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [
            {"alt": "Alpha file", "contentType": "FILE", "filename": "alpha-file.pdf", "originalSource": "https://cdn.example.com/alpha-file.pdf"},
            {"alt": "Beta image", "contentType": "IMAGE", "filename": "beta-image.jpg", "originalSource": "https://cdn.example.com/beta-image.jpg"}
        ]}),
    ));
    assert_eq!(
        create_files.body["data"]["fileCreate"]["userErrors"],
        json!([])
    );

    let create_saved_search = proxy.process_request(json_graphql_request(
        r#"
        mutation FileSavedSearchCreate($input: SavedSearchCreateInput!) {
          savedSearchCreate(input: $input) {
            savedSearch { id name query resourceType }
            userErrors { field message }
          }
        }
        "#,
        json!({"input": {
            "resourceType": "FILE",
            "name": "Alpha files",
            "query": "filename:alpha-file.pdf"
        }}),
    ));
    assert_eq!(
        create_saved_search.body["data"]["savedSearchCreate"]["userErrors"],
        json!([])
    );
    let saved_search_id = create_saved_search.body["data"]["savedSearchCreate"]["savedSearch"]
        ["id"]
        .as_str()
        .unwrap()
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query FileSavedSearchRead($savedSearchId: ID!) {
          files(first: 10, savedSearchId: $savedSearchId) {
            nodes { filename contentType }
          }
          fileSavedSearches(first: 10) {
            nodes { id name query resourceType }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"savedSearchId": saved_search_id}),
    ));

    assert_eq!(
        read.body["data"]["fileSavedSearches"]["nodes"],
        json!([{
            "id": saved_search_id,
            "name": "Alpha files",
            "query": "filename:alpha-file.pdf",
            "resourceType": "FILE"
        }])
    );
    assert_eq!(
        read.body["data"]["files"]["nodes"],
        json!([{"filename": "alpha-file.pdf", "contentType": "FILE"}])
    );
}

#[test]
fn media_files_connection_paginates_edges_nodes_and_page_info_consistently() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation FileReferenceCreate($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [
            {"alt": "First", "contentType": "IMAGE", "filename": "first.jpg", "originalSource": "https://cdn.example.com/first.jpg"},
            {"alt": "Second", "contentType": "IMAGE", "filename": "second.jpg", "originalSource": "https://cdn.example.com/second.jpg"},
            {"alt": "Third", "contentType": "IMAGE", "filename": "third.jpg", "originalSource": "https://cdn.example.com/third.jpg"}
        ]}),
    ));
    assert_eq!(create.body["data"]["fileCreate"]["userErrors"], json!([]));

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query FileReferenceFilesRead($first: Int!) {
          files(first: $first) {
            nodes { id alt }
            edges { cursor node { id alt } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"first": 2}),
    ));
    assert_eq!(
        first_page.body["data"]["files"],
        json!({
            "nodes": [
                {"id": "gid://shopify/MediaImage/2", "alt": "First"},
                {"id": "gid://shopify/MediaImage/3", "alt": "Second"}
            ],
            "edges": [
                {"cursor": "cursor:gid://shopify/MediaImage/2", "node": {"id": "gid://shopify/MediaImage/2", "alt": "First"}},
                {"cursor": "cursor:gid://shopify/MediaImage/3", "node": {"id": "gid://shopify/MediaImage/3", "alt": "Second"}}
            ],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": "cursor:gid://shopify/MediaImage/2",
                "endCursor": "cursor:gid://shopify/MediaImage/3"
            }
        })
    );

    let second_page = proxy.process_request(json_graphql_request(
        r#"
        query FileReferenceFilesRead($first: Int!, $after: String!) {
          files(first: $first, after: $after) {
            nodes { id alt }
            edges { cursor node { id alt } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"first": 2, "after": first_page.body["data"]["files"]["pageInfo"]["endCursor"]}),
    ));
    assert_eq!(
        second_page.body["data"]["files"],
        json!({
            "nodes": [{"id": "gid://shopify/MediaImage/4", "alt": "Third"}],
            "edges": [{"cursor": "cursor:gid://shopify/MediaImage/4", "node": {"id": "gid://shopify/MediaImage/4", "alt": "Third"}}],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": "cursor:gid://shopify/MediaImage/4",
                "endCursor": "cursor:gid://shopify/MediaImage/4"
            }
        })
    );

    let before_tail = proxy.process_request(json_graphql_request(
        r#"
        query FileReferenceFilesRead($last: Int!, $before: String!) {
          files(last: $last, before: $before) {
            nodes { id alt }
            edges { cursor node { id alt } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"last": 1, "before": "cursor:gid://shopify/MediaImage/4"}),
    ));
    assert_eq!(
        before_tail.body["data"]["files"],
        json!({
            "nodes": [{"id": "gid://shopify/MediaImage/3", "alt": "Second"}],
            "edges": [{"cursor": "cursor:gid://shopify/MediaImage/3", "node": {"id": "gid://shopify/MediaImage/3", "alt": "Second"}}],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": true,
                "startCursor": "cursor:gid://shopify/MediaImage/3",
                "endCursor": "cursor:gid://shopify/MediaImage/3"
            }
        })
    );
}

#[test]
fn media_file_create_allocates_unique_ids_across_separate_calls() {
    let mut proxy = snapshot_proxy();

    let create_query = r#"
        mutation FileReferenceCreate($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id alt createdAt fileStatus filename }
            userErrors { field message code }
          }
        }
        "#;

    let first = proxy.process_request(json_graphql_request(
        create_query,
        json!({"files": [{"alt": "First batch", "contentType": "IMAGE", "filename": "first.jpg", "originalSource": "https://cdn.example.com/first.jpg"}]}),
    ));
    let second = proxy.process_request(json_graphql_request(
        create_query,
        json!({"files": [{"alt": "Second batch", "contentType": "IMAGE", "filename": "second.jpg", "originalSource": "https://cdn.example.com/second.jpg"}]}),
    ));

    let first_id = first.body["data"]["fileCreate"]["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let second_id = second.body["data"]["fileCreate"]["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_ne!(first_id, second_id);
    assert_eq!(first_id, "gid://shopify/MediaImage/2");
    assert_eq!(second_id, "gid://shopify/MediaImage/4");
    assert_eq!(first.body["data"]["fileCreate"]["userErrors"], json!([]));
    assert_eq!(second.body["data"]["fileCreate"]["userErrors"], json!([]));

    let files_read = proxy.process_request(json_graphql_request(
        r#"
        query FileReferenceFilesRead {
          files(first: 10) {
            nodes { id alt createdAt fileStatus filename }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        files_read.body["data"]["files"]["nodes"],
        json!([
            {"id": first_id, "alt": "First batch", "createdAt": "2024-01-01T00:00:01.000Z", "fileStatus": "READY", "filename": "first.jpg"},
            {"id": second_id, "alt": "Second batch", "createdAt": "2024-01-01T00:00:01.000Z", "fileStatus": "READY", "filename": "second.jpg"}
        ])
    );
}

fn assert_file_create_batch_timestamps(batch_size: usize, expected_last_created_at: &str) {
    let mut proxy = snapshot_proxy();
    let files = (0..batch_size)
        .map(|index| {
            json!({
                "alt": format!("Batch file {index}"),
                "contentType": "IMAGE",
                "filename": format!("batch-file-{index}.jpg"),
                "originalSource": format!("https://cdn.example.com/batch-file-{index}.jpg")
            })
        })
        .collect::<Vec<_>>();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaFileCreateBatchTimestamps($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id createdAt updatedAt fileStatus }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "files": files }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["fileCreate"]["userErrors"], json!([]));

    let created_files = create.body["data"]["fileCreate"]["files"]
        .as_array()
        .expect("fileCreate should return files");
    assert_eq!(created_files.len(), batch_size);

    for (index, file) in created_files.iter().enumerate() {
        let expected_offset_seconds = u32::try_from(index + 1).unwrap();
        assert_valid_synthetic_media_timestamp(&file["createdAt"], expected_offset_seconds);
        assert_valid_synthetic_media_timestamp(&file["updatedAt"], expected_offset_seconds);
        assert_eq!(file["createdAt"], file["updatedAt"]);
    }
    assert_eq!(
        created_files.last().unwrap()["createdAt"],
        json!(expected_last_created_at)
    );
}

fn assert_valid_synthetic_media_timestamp(value: &Value, expected_offset_seconds: u32) {
    let timestamp = value
        .as_str()
        .expect("fileCreate timestamp should be a string");
    assert_eq!(timestamp.len(), "2024-01-01T00:00:00.000Z".len());
    assert_eq!(&timestamp[0..11], "2024-01-01T");
    assert_eq!(&timestamp[13..14], ":");
    assert_eq!(&timestamp[16..17], ":");
    assert_eq!(&timestamp[19..], ".000Z");

    let hour = timestamp[11..13].parse::<u32>().unwrap();
    let minute = timestamp[14..16].parse::<u32>().unwrap();
    let second = timestamp[17..19].parse::<u32>().unwrap();
    assert!(hour < 24, "hour should be valid in {timestamp}");
    assert!(minute < 60, "minute should be valid in {timestamp}");
    assert!(second < 60, "second should be valid in {timestamp}");
    assert_eq!(
        hour * 3600 + minute * 60 + second,
        expected_offset_seconds,
        "timestamp should advance deterministically by input index"
    );
}

#[test]
fn media_file_create_batch_timestamps_are_valid_for_large_batches() {
    assert_file_create_batch_timestamps(60, "2024-01-01T00:01:00.000Z");
    assert_file_create_batch_timestamps(250, "2024-01-01T00:04:10.000Z");
}

#[test]
fn media_file_delete_re_resolves_wrong_typed_gid_to_staged_media_image() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaFileDeleteTypedGidRoundtripCreate($files: [FileCreateInput!]!) {
          fileCreate(files: $files) { files { id alt createdAt fileStatus } userErrors { field message code } }
        }
        "#,
        json!({"files": [
            {"contentType": "IMAGE", "originalSource": "https://placehold.co/600x400/png", "alt": "Hermes typed delete actual 1777945543894"},
            {"contentType": "IMAGE", "originalSource": "https://placehold.co/600x400/png", "alt": "Hermes typed delete wrong type 1777945543894"}
        ]}),
    ));
    let actual_id = create.body["data"]["fileCreate"]["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let wrong_type_media_id = create.body["data"]["fileCreate"]["files"][1]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        create.body["data"]["fileCreate"],
        json!({"files": [
            {"id": actual_id, "alt": "Hermes typed delete actual 1777945543894", "createdAt": "2024-01-01T00:00:01.000Z", "fileStatus": "UPLOADED"},
            {"id": wrong_type_media_id, "alt": "Hermes typed delete wrong type 1777945543894", "createdAt": "2024-01-01T00:00:02.000Z", "fileStatus": "UPLOADED"}
        ], "userErrors": []})
    );

    let delete_actual = proxy.process_request(json_graphql_request(
        r#"
        mutation FileDeleteParity($fileIds: [ID!]!) {
          fileDelete(fileIds: $fileIds) { deletedFileIds userErrors { field message code } }
        }
        "#,
        json!({"fileIds": [actual_id]}),
    ));
    assert_eq!(
        delete_actual.body["data"]["fileDelete"],
        json!({"deletedFileIds": [actual_id], "userErrors": []})
    );

    let delete_wrong_type = proxy.process_request(json_graphql_request(
        r#"
        mutation FileDeleteParity($fileIds: [ID!]!) {
          fileDelete(fileIds: $fileIds) { deletedFileIds userErrors { field message code } }
        }
        "#,
        json!({"fileIds": [wrong_type_media_id.replace("/MediaImage/", "/Video/")]}),
    ));
    assert_eq!(
        delete_wrong_type.body["data"]["fileDelete"],
        json!({"deletedFileIds": [wrong_type_media_id], "userErrors": []})
    );
}

#[test]
fn media_file_create_validates_inputs_without_operation_name_guards() {
    let mut proxy = snapshot_proxy();
    let mutation = r#"
        mutation MediaFileCreateValidation($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id fileStatus }
            userErrors { field message code }
          }
        }
    "#;

    let data_url = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"originalSource": "data:image/png;base64,iVBORw0KGgo="}]}),
    ));
    assert_eq!(
        data_url.body["data"]["fileCreate"],
        json!({"files": [], "userErrors": [{
            "field": ["files", "0", "originalSource"],
            "message": "File URL is invalid",
            "code": "INVALID_IMAGE_SOURCE_URL"
        }]})
    );

    let extension_mismatch = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"originalSource": "https://cdn.example.com/source.png", "filename": "source.jpg"}]}),
    ));
    assert_eq!(
        extension_mismatch.body["data"]["fileCreate"],
        json!({"files": [], "userErrors": [{
            "field": ["files", "0", "filename"],
            "message": "Provided filename extension must match original source.",
            "code": "MISMATCHED_FILENAME_AND_ORIGINAL_SOURCE"
        }]})
    );

    let extension_case_mismatch = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"originalSource": "https://cdn.example.com/source.PNG", "filename": "source.png", "contentType": "IMAGE"}]}),
    ));
    assert_eq!(
        extension_case_mismatch.body["data"]["fileCreate"],
        json!({"files": [], "userErrors": [{
            "field": ["files", "0", "filename"],
            "message": "Provided filename extension must match original source.",
            "code": "MISMATCHED_FILENAME_AND_ORIGINAL_SOURCE"
        }]})
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    let read_after_rejections = proxy.process_request(json_graphql_request(
        r#"
        query MediaFileCreateValidationRead {
          files(first: 5) { nodes { id } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read_after_rejections.body["data"]["files"]["nodes"],
        json!([])
    );

    let duplicate_mode = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"originalSource": "https://cdn.example.com/source.png", "contentType": "IMAGE", "duplicateResolutionMode": "REPLACE"}]}),
    ));
    assert_eq!(
        duplicate_mode.body["data"]["fileCreate"],
        json!({"files": [], "userErrors": [{
            "field": ["files", "0", "filename"],
            "message": "Missing filename argument when attempting to use REPLACE duplicate mode.",
            "code": "MISSING_FILENAME_FOR_DUPLICATE_MODE_REPLACE"
        }]})
    );

    let success = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"originalSource": "https://cdn.example.com/source.png", "filename": "source.png", "contentType": "IMAGE"}]}),
    ));
    assert_eq!(
        success.body["data"]["fileCreate"],
        json!({"files": [{"id": "gid://shopify/MediaImage/2", "fileStatus": "UPLOADED"}], "userErrors": []})
    );
}

#[test]
fn media_file_create_omitted_model_extension_stages_generic_file_and_preserves_explicit_model3d() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaFileCreateModelExtensionInference($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files {
              __typename
              id
              alt
              fileStatus
              filename
              mimeType
              ... on GenericFile { url }
              ... on Model3d { mediaErrors mediaWarnings }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [
            {"originalSource": "https://cdn.example.com/model.glb", "alt": "Omitted GLB"},
            {"originalSource": "https://cdn.example.com/explicit-model.glb", "filename": "explicit-model.glb", "contentType": "MODEL_3D", "alt": "Explicit model"}
        ]}),
    ));
    assert_eq!(
        create.body["data"]["fileCreate"],
        json!({"files": [
            {
                "__typename": "GenericFile",
                "id": "gid://shopify/GenericFile/2",
                "alt": "Omitted GLB",
                "fileStatus": "UPLOADED",
                "filename": "model.glb",
                "mimeType": "model/gltf-binary",
                "url": "https://cdn.example.com/model.glb"
            },
            {
                "__typename": "Model3d",
                "id": "gid://shopify/Model3d/3",
                "alt": "Explicit model",
                "fileStatus": "UPLOADED",
                "filename": "explicit-model.glb",
                "mimeType": "model/gltf-binary",
                "mediaErrors": [],
                "mediaWarnings": []
            }
        ], "userErrors": []})
    );

    let files_read = proxy.process_request(json_graphql_request(
        r#"
        query MediaFileCreateModelExtensionInferenceFiles {
          files(first: 10) {
            nodes {
              __typename
              id
              filename
              ... on GenericFile { url }
              ... on Model3d { mediaErrors mediaWarnings }
            }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        files_read.body["data"]["files"]["nodes"],
        json!([
            {
                "__typename": "GenericFile",
                "id": "gid://shopify/GenericFile/2",
                "filename": "model.glb",
                "url": "https://cdn.example.com/model.glb"
            },
            {
                "__typename": "Model3d",
                "id": "gid://shopify/Model3d/3",
                "filename": "explicit-model.glb",
                "mediaErrors": [],
                "mediaWarnings": []
            }
        ])
    );

    let generic_node = proxy.process_request(json_graphql_request(
        r#"
        query MediaFileCreateModelExtensionInferenceGenericNode($id: ID!) {
          node(id: $id) {
            __typename
            id
            ... on GenericFile { alt fileStatus filename mimeType url }
            ... on Model3d { mediaErrors mediaWarnings }
          }
        }
        "#,
        json!({"id": "gid://shopify/GenericFile/2"}),
    ));
    assert_eq!(
        generic_node.body["data"]["node"],
        json!({
            "__typename": "GenericFile",
            "id": "gid://shopify/GenericFile/2",
            "alt": "Omitted GLB",
            "fileStatus": "READY",
            "filename": "model.glb",
            "mimeType": "model/gltf-binary",
            "url": "https://cdn.example.com/model.glb"
        })
    );

    let model_node = proxy.process_request(json_graphql_request(
        r#"
        query MediaFileCreateModelExtensionInferenceModelNode($id: ID!) {
          node(id: $id) {
            __typename
            id
            ... on GenericFile { url }
            ... on Model3d { alt fileStatus filename mimeType mediaErrors mediaWarnings }
          }
        }
        "#,
        json!({"id": "gid://shopify/Model3d/3"}),
    ));
    assert_eq!(
        model_node.body["data"]["node"],
        json!({
            "__typename": "Model3d",
            "id": "gid://shopify/Model3d/3",
            "alt": "Explicit model",
            "fileStatus": "READY",
            "filename": "explicit-model.glb",
            "mimeType": "model/gltf-binary",
            "mediaErrors": [],
            "mediaWarnings": []
        })
    );
}

#[test]
fn media_file_create_top_level_input_errors_do_not_stage_or_log() {
    let mut proxy = snapshot_proxy();
    let mutation = r#"
        mutation MediaFileCreateInputValidation($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id }
            userErrors { field message code }
          }
        }
    "#;

    let empty_source = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"originalSource": ""}]}),
    ));
    assert_eq!(empty_source.body["data"]["fileCreate"], Value::Null);
    assert_eq!(
        empty_source.body["errors"][0]["message"],
        json!("originalSource is too short (minimum is 1)")
    );
    assert_eq!(
        empty_source.body["errors"][0]["extensions"]["code"],
        json!("INVALID_FIELD_ARGUMENTS")
    );

    let too_many_files = (0..251)
        .map(|index| json!({"originalSource": format!("https://cdn.example.com/file-{index}.png")}))
        .collect::<Vec<_>>();
    let batch_size = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": too_many_files}),
    ));
    assert!(batch_size.body.get("data").is_none());
    assert_eq!(
        batch_size.body["errors"][0]["extensions"]["code"],
        json!("MAX_INPUT_SIZE_EXCEEDED")
    );
    assert_eq!(
        batch_size.body["errors"][0]["path"],
        json!(["fileCreate", "files"])
    );

    let log = proxy.process_request(Request {
        method: "GET".to_string(),
        path: "/__meta/log".to_string(),
        ..Default::default()
    });
    assert_eq!(log.body, json!({"entries": []}));
}

#[test]
fn media_file_update_hydrates_real_file_before_staging_captured_id() {
    let media_id = "gid://shopify/MediaImage/43688017887538";
    let upstream_bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_bodies = Arc::clone(&upstream_bodies);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            captured_bodies.lock().unwrap().push(body.clone());
            assert_eq!(
                body["variables"]["fileIds"],
                json!([media_id]),
                "fileUpdate hydrate should request the target file id"
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "nodes": [{
                            "id": media_id,
                            "__typename": "MediaImage",
                            "alt": "Hydrated alt",
                            "createdAt": "2026-06-04T00:00:00Z",
                            "fileStatus": "READY",
                            "image": {
                                "url": "https://cdn.example.com/hydrated-file-real.jpg",
                                "width": 640,
                                "height": 480
                            },
                            "preview": {
                                "image": {
                                    "url": "https://cdn.example.com/hydrated-file-real-preview.jpg",
                                    "width": 320,
                                    "height": 240
                                }
                            }
                        }]
                    }
                }),
            }
        });

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation FileUpdateHydratesCapturedId($files: [FileUpdateInput!]!) {
          fileUpdate(files: $files) {
            files {
              id
              alt
              fileStatus
              filename
              ... on MediaImage { image { url width height } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [{"id": media_id, "alt": "Updated hydrated alt"}]}),
    ));

    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["fileUpdate"],
        json!({
            "files": [{
                "id": media_id,
                "alt": "Updated hydrated alt",
                "fileStatus": "READY",
                "filename": "hydrated-file-real.jpg",
                "image": {
                    "url": "https://cdn.example.com/hydrated-file-real.jpg",
                    "width": 640,
                    "height": 480
                }
            }],
            "userErrors": []
        })
    );
    let bodies = upstream_bodies.lock().unwrap();
    assert_eq!(bodies.len(), 1);
    assert!(bodies[0]["query"]
        .as_str()
        .is_some_and(|query| query.contains("MediaFileUpdateHydrate")));
}

#[test]
fn media_file_update_rejects_filename_extension_case_mismatch_without_staging() {
    let media_id = "gid://shopify/MediaImage/43688017887538";
    let upstream_bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_bodies = Arc::clone(&upstream_bodies);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            captured_bodies.lock().unwrap().push(body.clone());
            let query = body["query"].as_str().unwrap_or_default();
            if query.contains("MediaFileUpdateHydrate") {
                assert_eq!(body["variables"]["fileIds"], json!([media_id]));
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "nodes": [{
                                "id": media_id,
                                "__typename": "MediaImage",
                                "alt": "Ready image",
                                "createdAt": "2026-06-05T00:00:00Z",
                                "fileStatus": "READY",
                                "image": {
                                    "url": "https://cdn.example.com/ready-image.JPG",
                                    "width": 640,
                                    "height": 480
                                },
                                "preview": {
                                    "image": {
                                        "url": "https://cdn.example.com/ready-image.JPG",
                                        "width": 320,
                                        "height": 240
                                    }
                                }
                            }]
                        }
                    }),
                };
            }
            assert!(query.contains("MediaFileUpdateRejectedRead"));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "files": {
                            "nodes": [{
                                "id": media_id,
                                "__typename": "MediaImage",
                                "alt": "Ready image",
                                "createdAt": "2026-06-05T00:00:00Z",
                                "fileStatus": "READY",
                                "filename": "ready-image.JPG",
                                "image": {
                                    "url": "https://cdn.example.com/ready-image.JPG",
                                    "width": 640,
                                    "height": 480
                                },
                                "preview": {
                                    "image": {
                                        "url": "https://cdn.example.com/ready-image.JPG",
                                        "width": 320,
                                        "height": 240
                                    }
                                }
                            }]
                        }
                    }
                }),
            }
        });

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaFileUpdateValidation($files: [FileUpdateInput!]!) {
          fileUpdate(files: $files) {
            files { id filename }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [{"id": media_id, "filename": "ready-image.jpg"}]}),
    ));
    assert_eq!(
        update.body["data"]["fileUpdate"],
        json!({"files": [], "userErrors": [{
            "field": ["files"],
            "message": "The filename extension provided must match the original filename.",
            "code": "INVALID_FILENAME_EXTENSION"
        }]})
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));
    let bodies = upstream_bodies.lock().unwrap();
    assert_eq!(bodies.len(), 1);
    assert!(bodies[0]["query"]
        .as_str()
        .is_some_and(|query| query.contains("MediaFileUpdateHydrate")));
    drop(bodies);

    let read_after_rejected_update = proxy.process_request(json_graphql_request(
        r#"
        query MediaFileUpdateRejectedRead {
          files(first: 5) { nodes { id filename } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read_after_rejected_update.body["data"]["files"]["nodes"],
        json!([{ "id": media_id, "filename": "ready-image.JPG" }])
    );
    let bodies = upstream_bodies.lock().unwrap();
    assert_eq!(bodies.len(), 2);
    assert!(bodies[1]["query"]
        .as_str()
        .is_some_and(|query| query.contains("MediaFileUpdateRejectedRead")));
}

#[test]
fn media_file_update_missing_id_coerces_before_staging() {
    let upstream_bodies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured_bodies = Arc::clone(&upstream_bodies);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            captured_bodies.lock().unwrap().push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": [Value::Null] } }),
            }
        });
    let mutation = r#"
        mutation MediaFileUpdateMissingId($files: [FileUpdateInput!]!) {
          fileUpdate(files: $files) {
            files { id }
            userErrors { field message code }
          }
        }
    "#;

    let omitted = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"alt": "new alt"}]}),
    ));
    assert_eq!(omitted.status, 200);
    assert_eq!(omitted.body.get("data"), None);
    assert_eq!(
        omitted.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        omitted.body["errors"][0]["extensions"]["problems"],
        json!([{ "path": [0, "id"], "explanation": "Expected value to not be null" }])
    );
    assert!(
        omitted.body["errors"][0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("0.id (Expected value to not be null)")),
        "{:?}",
        omitted.body["errors"][0]
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));

    let batch_omitted = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [
            {"id": "gid://shopify/MediaImage/404", "alt": "supplied"},
            {"alt": "missing id"}
        ]}),
    ));
    assert_eq!(batch_omitted.status, 200);
    assert_eq!(batch_omitted.body.get("data"), None);
    assert_eq!(
        batch_omitted.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        batch_omitted.body["errors"][0]["extensions"]["problems"],
        json!([{ "path": [1, "id"], "explanation": "Expected value to not be null" }])
    );
    assert_eq!(
        upstream_bodies.lock().unwrap().len(),
        0,
        "coercion should reject the whole fileUpdate list before resolver hydration"
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));

    let inline_omitted = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaFileUpdateInlineMissingId {
          fileUpdate(files: [{ alt: "new alt" }]) {
            files { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(inline_omitted.status, 200);
    assert_eq!(inline_omitted.body.get("data"), None);
    assert_eq!(
        inline_omitted.body["errors"][0]["message"],
        json!("Argument 'id' on InputObject 'FileUpdateInput' is required. Expected type ID!")
    );
    assert_eq!(
        inline_omitted.body["errors"][0]["extensions"]["code"],
        json!("missingRequiredInputObjectAttribute")
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));

    let supplied_missing = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"id": "gid://shopify/MediaImage/404", "alt": "missing"}]}),
    ));
    assert_eq!(
        supplied_missing.body["data"]["fileUpdate"],
        json!({"files": [], "userErrors": [{
            "field": ["files"],
            "message": "File id [\"gid://shopify/MediaImage/404\"] does not exist.",
            "code": "FILE_DOES_NOT_EXIST"
        }]})
    );
}

#[test]
fn media_file_update_validates_core_bucket_ordering() {
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(|request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream body parses");
            let nodes = body["variables"]["fileIds"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|id| match id.as_str() {
                    Some("gid://shopify/MediaImage/43688017887538") => json!({
                        "id": "gid://shopify/MediaImage/43688017887538",
                        "__typename": "MediaImage",
                        "alt": "Ready image",
                        "createdAt": "2026-06-05T00:00:00Z",
                        "fileStatus": "READY",
                        "image": {
                            "url": "https://cdn.example.com/ready-image.jpg",
                            "width": 640,
                            "height": 480
                        },
                        "preview": {
                            "image": {
                                "url": "https://cdn.example.com/ready-image-preview.jpg",
                                "width": 320,
                                "height": 240
                            }
                        }
                    }),
                    Some("gid://shopify/ExternalVideo/43688017953074") => json!({
                        "id": "gid://shopify/ExternalVideo/43688017953074",
                        "__typename": "ExternalVideo",
                        "alt": "Ready external video",
                        "createdAt": "2026-06-05T00:00:00Z",
                        "fileStatus": "READY"
                    }),
                    _ => Value::Null,
                })
                .collect::<Vec<_>>();
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": nodes } }),
            }
        });
    let mutation = r#"
        mutation MediaFileUpdateValidation($files: [FileUpdateInput!]!) {
          fileUpdate(files: $files) {
            files { id fileStatus alt }
            userErrors { field message code }
          }
        }
    "#;
    let long_alt = "a".repeat(513);

    let missing_with_source_conflict = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"id": "gid://shopify/MediaImage/404", "originalSource": "https://cdn.example.com/source.png", "previewImageSource": "https://cdn.example.com/preview.png"}]}),
    ));
    assert_eq!(
        missing_with_source_conflict.body["data"]["fileUpdate"],
        json!({"files": [], "userErrors": [{
            "field": ["files"],
            "message": "File id [\"gid://shopify/MediaImage/404\"] does not exist.",
            "code": "FILE_DOES_NOT_EXIST"
        }]})
    );

    let missing_with_long_alt = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"id": "gid://shopify/MediaImage/404", "alt": long_alt.clone()}]}),
    ));
    assert_eq!(
        missing_with_long_alt.body["data"]["fileUpdate"],
        json!({"files": [], "userErrors": [{
            "field": ["files"],
            "message": "File id [\"gid://shopify/MediaImage/404\"] does not exist.",
            "code": "FILE_DOES_NOT_EXIST"
        }]})
    );

    let unknown_version_field = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"id": "gid://shopify/MediaImage/404", "originalSource": "https://cdn.example.com/source.png", "revertToVersionId": "gid://shopify/FileVersion/9"}]}),
    ));
    assert_eq!(
        unknown_version_field.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        unknown_version_field.body["errors"][0]["extensions"]["problems"],
        json!([{
            "path": [0, "revertToVersionId"],
            "explanation": "Field is not defined on FileUpdateInput"
        }])
    );
    assert_eq!(unknown_version_field.body.get("data"), None);

    let unknown_version_field_on_external_video = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"id": "gid://shopify/ExternalVideo/43688017953074", "originalSource": "https://cdn.example.com/source.mp4", "revertToVersionId": "gid://shopify/FileVersion/9"}]}),
    ));
    assert_eq!(
        unknown_version_field_on_external_video.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        unknown_version_field_on_external_video.body["errors"][0]["extensions"]["problems"],
        json!([{
            "path": [0, "revertToVersionId"],
            "explanation": "Field is not defined on FileUpdateInput"
        }])
    );
    assert_eq!(
        unknown_version_field_on_external_video.body.get("data"),
        None
    );

    let missing = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [
            {"id": "gid://shopify/MediaImage/404", "alt": "Missing one"},
            {"id": "gid://shopify/MediaImage/405", "alt": "Missing two"}
        ]}),
    ));
    assert_eq!(
        missing.body["data"]["fileUpdate"],
        json!({"files": [], "userErrors": [{
            "field": ["files"],
            "message": "File ids [\"gid://shopify/MediaImage/404\", \"gid://shopify/MediaImage/405\"] do not exist.",
            "code": "FILE_DOES_NOT_EXIST"
        }]})
    );

    let create_non_ready = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaFileCreateForUpdateValidation($files: [FileCreateInput!]!) {
          fileCreate(files: $files) { files { id fileStatus } userErrors { code } }
        }
        "#,
        json!({"files": [{"originalSource": "https://cdn.example.com/non-ready.png", "contentType": "IMAGE"}]}),
    ));
    let non_ready_id = create_non_ready.body["data"]["fileCreate"]["files"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let non_ready_with_source_conflict = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"id": non_ready_id, "alt": long_alt, "originalSource": "https://cdn.example.com/source.png", "previewImageSource": "https://cdn.example.com/preview.png"}]}),
    ));
    assert_eq!(
        non_ready_with_source_conflict.body["data"]["fileUpdate"],
        json!({"files": [], "userErrors": [{
            "field": ["files"],
            "message": "Non-ready files cannot be updated.",
            "code": "NON_READY_STATE"
        }]})
    );

    let ready_source_conflict = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"id": "gid://shopify/MediaImage/43688017887538", "originalSource": "https://cdn.example.com/source.png", "previewImageSource": "https://cdn.example.com/preview.png"}]}),
    ));
    assert_eq!(
        ready_source_conflict.body["data"]["fileUpdate"],
        json!({"files": [], "userErrors": [
            {
                "field": ["files", "0", "previewImageSource"],
                "message": "Cannot update the preview image and image at the same time because they are one and the same.",
                "code": "INVALID"
            },
            {
                "field": ["files", "0", "originalSource"],
                "message": "Cannot update the preview image and image at the same time because they are one and the same.",
                "code": "INVALID"
            }
        ]})
    );
}

#[test]
fn media_staged_uploads_create_validates_file_size_mime_and_omits_user_error_code() {
    let mut proxy = snapshot_proxy();
    let mutation = r#"
        mutation MediaStagedUploadsCreateValidation($input: [StagedUploadInput!]!) {
          stagedUploadsCreate(input: $input) {
            stagedTargets { url resourceUrl parameters { name value } }
            userErrors { field message }
          }
        }
    "#;

    let missing_video_size = proxy.process_request(json_graphql_request(
        mutation,
        json!({"input": [{"resource": "VIDEO", "filename": "clip.mp4", "mimeType": "video/mp4"}]}),
    ));
    assert_eq!(
        missing_video_size.body["data"]["stagedUploadsCreate"],
        json!({"stagedTargets": [{"url": null, "resourceUrl": null, "parameters": []}], "userErrors": [{
            "field": ["input", "0", "fileSize"],
            "message": "file size is required for video resources"
        }]})
    );

    let missing_model_size = proxy.process_request(json_graphql_request(
        mutation,
        json!({"input": [{"resource": "MODEL_3D", "filename": "chair.glb", "mimeType": "model/gltf-binary"}]}),
    ));
    assert_eq!(
        missing_model_size.body["data"]["stagedUploadsCreate"],
        json!({"stagedTargets": [{"url": null, "resourceUrl": null, "parameters": []}], "userErrors": [{
            "field": ["input", "0", "fileSize"],
            "message": "file size is required for 3D model resources"
        }]})
    );

    let bad_image_mime = proxy.process_request(json_graphql_request(
        mutation,
        json!({"input": [{"resource": "IMAGE", "filename": "image.exe", "mimeType": "application/x-msdownload"}]}),
    ));
    assert_eq!(
        bad_image_mime.body["data"]["stagedUploadsCreate"],
        json!({"stagedTargets": [{"url": null, "resourceUrl": null, "parameters": []}], "userErrors": [{
            "field": ["input", "0", "mimeType"],
            "message": "image.exe: (application/x-msdownload) is not a recognized format"
        }]})
    );
}

#[test]
fn media_staged_uploads_create_missing_required_filename_or_mime_type_coerces_before_staging() {
    let mut proxy = snapshot_proxy();

    let inline_missing = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaStagedUploadsCreateMissingRequiredArgs {
          stagedUploadsCreate(input: [{ resource: FILE }]) {
            stagedTargets { url resourceUrl }
            userErrors { field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(inline_missing.status, 200);
    assert_eq!(inline_missing.body.get("data"), None);
    assert_eq!(
        inline_missing.body["errors"],
        json!([
            {
                "message": "Argument 'filename' on InputObject 'StagedUploadInput' is required. Expected type String!",
                "locations": [{ "line": 3, "column": 39 }],
                "path": [
                    "mutation MediaStagedUploadsCreateMissingRequiredArgs",
                    "stagedUploadsCreate",
                    "input",
                    0,
                    "filename"
                ],
                "extensions": {
                    "code": "missingRequiredInputObjectAttribute",
                    "argumentName": "filename",
                    "argumentType": "String!",
                    "inputObjectType": "StagedUploadInput"
                }
            },
            {
                "message": "Argument 'mimeType' on InputObject 'StagedUploadInput' is required. Expected type String!",
                "locations": [{ "line": 3, "column": 39 }],
                "path": [
                    "mutation MediaStagedUploadsCreateMissingRequiredArgs",
                    "stagedUploadsCreate",
                    "input",
                    0,
                    "mimeType"
                ],
                "extensions": {
                    "code": "missingRequiredInputObjectAttribute",
                    "argumentName": "mimeType",
                    "argumentType": "String!",
                    "inputObjectType": "StagedUploadInput"
                }
            }
        ])
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));

    let variable_missing_mime_type = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaStagedUploadsCreateVariableMissingMimeType($input: [StagedUploadInput!]!) {
          stagedUploadsCreate(input: $input) {
            stagedTargets { url resourceUrl parameters { name value } }
            userErrors { field message }
          }
        }
        "#,
        json!({"input": [{"resource": "FILE", "filename": "required-args.txt"}]}),
    ));
    assert_eq!(variable_missing_mime_type.status, 200);
    assert_eq!(variable_missing_mime_type.body.get("data"), None);
    assert_eq!(
        variable_missing_mime_type.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert_eq!(
        variable_missing_mime_type.body["errors"][0]["extensions"]["problems"],
        json!([{ "path": [0, "mimeType"], "explanation": "Expected value to not be null" }])
    );
    assert!(
        variable_missing_mime_type.body["errors"][0]["message"]
            .as_str()
            .is_some_and(|message| message.contains("0.mimeType (Expected value to not be null)")),
        "{:?}",
        variable_missing_mime_type.body["errors"][0]
    );
    assert_eq!(log_snapshot(&proxy)["entries"], json!([]));

    let fully_specified = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaStagedUploadsCreateFullySpecified($input: [StagedUploadInput!]!) {
          stagedUploadsCreate(input: $input) {
            stagedTargets { url resourceUrl parameters { name value } }
            userErrors { field message }
          }
        }
        "#,
        json!({"input": [{"resource": "FILE", "filename": "required-args.txt", "mimeType": "text/plain"}]}),
    ));
    assert_eq!(fully_specified.status, 200);
    assert_eq!(
        fully_specified.body["data"]["stagedUploadsCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        fully_specified.body["data"]["stagedUploadsCreate"]["stagedTargets"][0]["parameters"][0],
        json!({"name": "content_type", "value": "text/plain"})
    );
    assert_eq!(
        fully_specified.body["data"]["stagedUploadsCreate"]["stagedTargets"][0]["resourceUrl"]
            .as_str()
            .map(|url| url.ends_with("/required-args.txt")),
        Some(true)
    );
}

#[test]
fn media_file_acknowledge_update_failed_validates_missing_and_non_ready_ids() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaFileCreateForAck($files: [FileCreateInput!]!) {
          fileCreate(files: $files) { files { id fileStatus } userErrors { code } }
        }
        "#,
        json!({"files": [{"originalSource": "https://cdn.example.com/non-ready.png", "contentType": "IMAGE"}]}),
    ));
    assert_eq!(
        create.body["data"]["fileCreate"]["files"][0]["id"],
        json!("gid://shopify/MediaImage/2")
    );

    let acknowledge_non_ready = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaFileAcknowledgeValidation($fileIds: [ID!]!) {
          fileAcknowledgeUpdateFailed(fileIds: $fileIds) {
            files { id fileStatus }
            userErrors { field message code }
          }
        }
        "#,
        json!({"fileIds": ["gid://shopify/MediaImage/2"]}),
    ));
    assert_eq!(
        acknowledge_non_ready.body["data"]["fileAcknowledgeUpdateFailed"],
        json!({"files": null, "userErrors": [{
            "field": ["fileIds"],
            "message": "File with id gid://shopify/MediaImage/2 is not in the READY state.",
            "code": "NON_READY_STATE"
        }]})
    );

    let acknowledge_missing = proxy.process_request(json_graphql_request(
        r#"
        mutation MediaFileAcknowledgeValidation($fileIds: [ID!]!) {
          fileAcknowledgeUpdateFailed(fileIds: $fileIds) {
            files { id fileStatus }
            userErrors { field message code }
          }
        }
        "#,
        json!({"fileIds": ["gid://shopify/MediaImage/999", "gid://shopify/MediaImage/2"]}),
    ));
    assert_eq!(
        acknowledge_missing.body["data"]["fileAcknowledgeUpdateFailed"],
        json!({"files": null, "userErrors": [{
            "field": ["fileIds"],
            "message": "File id gid://shopify/MediaImage/999 does not exist.",
            "code": "FILE_DOES_NOT_EXIST"
        }]})
    );

    let downstream_read = proxy.process_request(json_graphql_request(
        r#"
        query MediaFileAcknowledgeValidationRead {
          files(first: 5) {
            nodes {
              id
              fileStatus
              __typename
              mediaErrors { code message }
              mediaWarnings { code message }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        downstream_read.body["data"]["files"],
        json!({
            "nodes": [{
                "id": "gid://shopify/MediaImage/2",
                "fileStatus": "READY",
                "__typename": "MediaImage",
                "mediaErrors": [],
                "mediaWarnings": []
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "cursor:gid://shopify/MediaImage/2",
                "endCursor": "cursor:gid://shopify/MediaImage/2"
            }
        })
    );
}

#[test]
fn media_file_create_and_update_reference_authorization_is_top_level_access_denied() {
    let mut proxy = snapshot_proxy();
    let create_query = r#"
        mutation MediaReferenceAuthCreate($files: [FileCreateInput!]!) {
          fileCreate(files: $files) { files { id } userErrors { field message code } }
        }
    "#;
    let mut create_request = json_graphql_request(
        create_query,
        json!({"files": [{
            "originalSource": "https://cdn.example.com/reference.png",
            "referencesToAdd": ["gid://shopify/Product/1"]
        }]}),
    );
    create_request.headers.insert(
        "x-shopify-draft-proxy-manage-products".to_string(),
        "false".to_string(),
    );
    let create = proxy.process_request(create_request);
    assert_eq!(create.body["data"]["fileCreate"], Value::Null);
    assert_eq!(
        create.body["errors"][0],
        json!({
            "message": "Access denied: Missing permission to manage products.",
            "locations": [{"line": 2, "column": 3}],
            "extensions": {
                "code": "ACCESS_DENIED",
                "documentation": "https://shopify.dev/api/usage/access-scopes"
            },
            "path": ["fileCreate"]
        })
    );

    let update_query = r#"
        mutation MediaReferenceAuthUpdate($files: [FileUpdateInput!]!) {
          fileUpdate(files: $files) { files { id } userErrors { field message code } }
        }
    "#;
    let mut update_request = json_graphql_request(
        update_query,
        json!({"files": [{
            "id": "gid://shopify/MediaImage/43693628424498",
            "referencesToAdd": ["gid://shopify/Product/1"]
        }]}),
    );
    update_request.headers.insert(
        "x-shopify-draft-proxy-manage-products".to_string(),
        "no".to_string(),
    );
    let update = proxy.process_request(update_request);
    assert_eq!(update.body["data"]["fileUpdate"], Value::Null);
    assert_eq!(
        update.body["errors"][0]["extensions"]["code"],
        json!("ACCESS_DENIED")
    );
    assert_eq!(update.body["errors"][0]["path"], json!(["fileUpdate"]));
}

#[test]
fn media_file_create_quota_affordance_rejects_matching_non_image_inputs() {
    let mut proxy = snapshot_proxy();
    let mut request = json_graphql_request(
        r#"
        mutation MediaQuota($files: [FileCreateInput!]!) {
          fileCreate(files: $files) {
            files { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({"files": [
            {"originalSource": "https://cdn.example.com/video.mp4", "contentType": "VIDEO"},
            {"originalSource": "https://cdn.example.com/model.glb", "contentType": "MODEL_3D"},
            {"originalSource": "https://cdn.example.com/file.txt", "contentType": "FILE"}
        ]}),
    );
    request.headers.insert(
        "x-shopify-draft-proxy-media-quota-errors".to_string(),
        "VIDEO_THROTTLE_EXCEEDED,MODEL3D_THROTTLE_EXCEEDED,NON_IMAGE_MEDIA_PER_SHOP_LIMIT_EXCEEDED"
            .to_string(),
    );
    let response = proxy.process_request(request);
    assert_eq!(
        response.body["data"]["fileCreate"],
        json!({"files": [], "userErrors": [
            {
                "field": ["files", "0", "contentType"],
                "message": "Video upload throttle exceeded.",
                "code": "VIDEO_THROTTLE_EXCEEDED"
            },
            {
                "field": ["files", "1", "contentType"],
                "message": "Model 3D upload throttle exceeded.",
                "code": "MODEL3D_THROTTLE_EXCEEDED"
            },
            {
                "field": ["files", "2", "contentType"],
                "message": "Non-image media per shop limit exceeded.",
                "code": "NON_IMAGE_MEDIA_PER_SHOP_LIMIT_EXCEEDED"
            }
        ]})
    );
}

#[test]
fn online_store_content_lifecycle_dispatches_by_root_and_reads_staged_state() {
    let upstream_calls = Arc::new(Mutex::new(0_usize));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let upstream_calls = upstream_calls.clone();
        move |_request| {
            *upstream_calls.lock().unwrap() += 1;
            Response {
                status: 599,
                headers: Default::default(),
                body: json!({"errors": [{"message": "unexpected upstream call"}]}),
            }
        }
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation DeliberatelyNotAnOnlineStoreOperation($blog: BlogCreateInput!, $page: PageCreateInput!) {
          madeBlog: blogCreate(blog: $blog) {
            blog { id title handle commentPolicy createdAt updatedAt articlesCount { count precision } }
            userErrors { field message code }
          }
          madePage: pageCreate(page: $page) {
            page { id title handle body bodySummary isPublished publishedAt createdAt updatedAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "blog": {"title": "CMS Lifecycle Blog", "commentPolicy": "MODERATED"},
            "page": {"title": "CMS Lifecycle Page", "body": "<p>Hello <strong>page</strong></p>", "visible": false, "visibilityDate": "2099-01-01T00:00:00Z"}
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(create.body["data"]["madeBlog"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["madePage"]["userErrors"], json!([]));
    let blog_id = create.body["data"]["madeBlog"]["blog"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let blog_handle = create.body["data"]["madeBlog"]["blog"]["handle"]
        .as_str()
        .unwrap()
        .to_string();
    let blog_created_at = assert_online_store_operation_timestamp(
        &create.body["data"]["madeBlog"]["blog"]["createdAt"],
        "blogCreate.createdAt",
    );
    let blog_updated_at = assert_online_store_operation_timestamp(
        &create.body["data"]["madeBlog"]["blog"]["updatedAt"],
        "blogCreate.updatedAt",
    );
    assert_eq!(blog_created_at, blog_updated_at);
    let page_id = create.body["data"]["madePage"]["page"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let page_handle = create.body["data"]["madePage"]["page"]["handle"]
        .as_str()
        .unwrap()
        .to_string();
    let page_created_at = assert_online_store_operation_timestamp(
        &create.body["data"]["madePage"]["page"]["createdAt"],
        "pageCreate.createdAt",
    );
    let page_updated_at = assert_online_store_operation_timestamp(
        &create.body["data"]["madePage"]["page"]["updatedAt"],
        "pageCreate.updatedAt",
    );
    assert_eq!(page_created_at, page_updated_at);
    assert_eq!(
        create.body["data"]["madePage"]["page"]["body"],
        json!("<p>Hello <strong>page</strong></p>")
    );
    assert_eq!(
        create.body["data"]["madePage"]["page"]["bodySummary"],
        json!("Hello page")
    );
    assert_eq!(
        create.body["data"]["madePage"]["page"]["isPublished"],
        json!(false)
    );

    let article_create = proxy.process_request(json_graphql_request(
        r#"
        mutation AnotherUnrelatedOperationName($article: ArticleCreateInput!) {
          madeArticle: articleCreate(article: $article) {
            article { id title handle body summary tags isPublished publishedAt createdAt updatedAt author { name } blog { id title handle } commentsCount { count precision } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"article": {
            "title": "CMS Lifecycle Article",
            "body": "<p>Article body</p>",
            "summary": "Article summary",
            "tags": ["online-store", "cms"],
            "blogId": blog_id,
            "author": {"name": "CMS Author"},
            "isPublished": true
        }}),
    ));
    assert_eq!(article_create.status, 200);
    assert_eq!(
        article_create.body["data"]["madeArticle"]["userErrors"],
        json!([])
    );
    let article_id = article_create.body["data"]["madeArticle"]["article"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let article_handle = article_create.body["data"]["madeArticle"]["article"]["handle"]
        .as_str()
        .unwrap()
        .to_string();
    let article_created_at = assert_online_store_operation_timestamp(
        &article_create.body["data"]["madeArticle"]["article"]["createdAt"],
        "articleCreate.createdAt",
    );
    let article_updated_at = assert_online_store_operation_timestamp(
        &article_create.body["data"]["madeArticle"]["article"]["updatedAt"],
        "articleCreate.updatedAt",
    );
    let article_published_at = assert_online_store_operation_timestamp(
        &article_create.body["data"]["madeArticle"]["article"]["publishedAt"],
        "articleCreate.publishedAt",
    );
    assert_eq!(article_created_at, article_updated_at);
    assert_eq!(article_created_at, article_published_at);

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ReadStagedCms($blogId: ID!, $pageId: ID!, $articleId: ID!) {
          blog(id: $blogId) {
            id
            title
            createdAt
            updatedAt
            articlesCount { count precision }
            articles(first: 5) { nodes { id title handle } pageInfo { hasNextPage hasPreviousPage } }
          }
          page(id: $pageId) { id title handle isPublished createdAt updatedAt }
          article(id: $articleId) { id title isPublished publishedAt createdAt updatedAt blog { id title } commentsCount { count precision } }
          articleTags(limit: 10)
          blogsCount { count precision }
          pagesCount { count precision }
        }
        "#,
        json!({"blogId": blog_id, "pageId": page_id, "articleId": article_id}),
    ));
    assert_eq!(
        read.body["data"]["blog"]["articlesCount"],
        json!({"count": 1, "precision": "EXACT"})
    );
    assert_eq!(
        read.body["data"]["blog"]["articles"]["nodes"][0]["id"],
        json!(article_id)
    );
    assert_eq!(read.body["data"]["page"]["isPublished"], json!(false));
    assert_eq!(
        read.body["data"]["blog"]["createdAt"],
        json!(blog_created_at)
    );
    assert_eq!(
        read.body["data"]["blog"]["updatedAt"],
        json!(blog_updated_at)
    );
    assert_eq!(
        read.body["data"]["page"]["createdAt"],
        json!(page_created_at)
    );
    assert_eq!(
        read.body["data"]["page"]["updatedAt"],
        json!(page_updated_at)
    );
    assert_eq!(
        read.body["data"]["article"]["createdAt"],
        json!(article_created_at)
    );
    assert_eq!(
        read.body["data"]["article"]["updatedAt"],
        json!(article_updated_at)
    );
    assert_eq!(
        read.body["data"]["article"]["publishedAt"],
        json!(article_published_at)
    );
    assert_eq!(
        read.body["data"]["articleTags"],
        json!(["cms", "online-store"])
    );
    assert_eq!(
        read.body["data"]["blogsCount"],
        json!({"count": 1, "precision": "EXACT"})
    );
    assert_eq!(
        read.body["data"]["pagesCount"],
        json!({"count": 1, "precision": "EXACT"})
    );
    let node_read = proxy.process_request(json_graphql_request(
        r#"
        query ReadStagedCmsNode($articleId: ID!) {
          articleNode: node(id: $articleId) { __typename ... on Article { id title blog { id title } } }
        }
        "#,
        json!({"articleId": article_id}),
    ));
    assert_eq!(
        node_read.body["data"]["articleNode"]["__typename"],
        json!("Article")
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateCmsWithoutHandles(
          $blogId: ID!
          $pageId: ID!
          $articleId: ID!
          $blog: BlogUpdateInput!
          $page: PageUpdateInput!
          $article: ArticleUpdateInput!
        ) {
          blogUpdate(id: $blogId, blog: $blog) {
            blog { id title handle }
            userErrors { field message code }
          }
          pageUpdate(id: $pageId, page: $page) {
            page { id title handle }
            userErrors { field message code }
          }
          articleUpdate(id: $articleId, article: $article) {
            article { id title handle blog { id title handle } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "blogId": blog_id,
            "pageId": page_id,
            "articleId": article_id,
            "blog": {"title": "CMS Lifecycle Blog Updated"},
            "page": {"title": "CMS Lifecycle Page Updated"},
            "article": {"title": "CMS Lifecycle Article Updated"}
        }),
    ));
    assert_eq!(update.body["data"]["blogUpdate"]["userErrors"], json!([]));
    assert_eq!(update.body["data"]["pageUpdate"]["userErrors"], json!([]));
    assert_eq!(
        update.body["data"]["articleUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["blogUpdate"]["blog"]["handle"],
        json!(blog_handle)
    );
    assert_eq!(
        update.body["data"]["pageUpdate"]["page"]["handle"],
        json!(page_handle)
    );
    assert_eq!(
        update.body["data"]["articleUpdate"]["article"]["handle"],
        json!(article_handle)
    );
    assert_eq!(
        update.body["data"]["articleUpdate"]["article"]["blog"],
        json!({
            "id": blog_id,
            "title": "CMS Lifecycle Blog Updated",
            "handle": blog_handle
        })
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteCms($articleId: ID!, $pageId: ID!, $blogId: ID!) {
          articleDelete(id: $articleId) { deletedArticleId userErrors { field message code } }
          pageDelete(id: $pageId) { deletedPageId userErrors { field message code } }
          blogDelete(id: $blogId) { deletedBlogId userErrors { field message code } }
        }
        "#,
        json!({"articleId": article_id, "pageId": page_id, "blogId": blog_id}),
    ));
    assert_eq!(
        delete.body["data"]["articleDelete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        delete.body["data"]["articleDelete"]["deletedArticleId"],
        json!(article_id)
    );

    let read_after_delete = proxy.process_request(json_graphql_request(
        r#"
        query ReadDeletedCms($blogId: ID!, $pageId: ID!, $articleId: ID!) {
          blog(id: $blogId) { id }
          page(id: $pageId) { id }
          article(id: $articleId) { id }
          node(id: $articleId) { id }
          blogs(first: 10) { nodes { id } }
          pages(first: 10) { nodes { id } }
          articles(first: 10) { nodes { id } }
        }
        "#,
        json!({"blogId": blog_id, "pageId": page_id, "articleId": article_id}),
    ));
    assert_eq!(read_after_delete.body["data"]["blog"], Value::Null);
    assert_eq!(read_after_delete.body["data"]["page"], Value::Null);
    assert_eq!(read_after_delete.body["data"]["article"], Value::Null);
    assert_eq!(read_after_delete.body["data"]["node"], Value::Null);
    assert_eq!(read_after_delete.body["data"]["blogs"]["nodes"], json!([]));
    assert_eq!(read_after_delete.body["data"]["pages"]["nodes"], json!([]));
    assert_eq!(
        read_after_delete.body["data"]["articles"]["nodes"],
        json!([])
    );
    assert_eq!(*upstream_calls.lock().unwrap(), 2);
}

#[test]
fn online_store_nested_blog_articles_connection_honors_args_and_staged_writes() {
    let mut proxy = snapshot_proxy();

    let blog = proxy.process_request(json_graphql_request(
        r#"
        mutation NestedArticleBlog {
          blogCreate(blog: { title: "Nested article blog" }) {
            blog { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(blog.body["data"]["blogCreate"]["userErrors"], json!([]));
    let blog_id = blog.body["data"]["blogCreate"]["blog"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let mut article_ids = Vec::<(String, String)>::new();
    for title in [
        "Alpha nested article",
        "Bravo nested article",
        "Charlie nested article",
    ] {
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation NestedArticleCreate($article: ArticleCreateInput!) {
              articleCreate(article: $article) {
                article { id title }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "article": {
                    "title": title,
                    "blogId": blog_id,
                    "author": { "name": "Nested Author" }
                }
            }),
        ));
        assert_eq!(
            create.body["data"]["articleCreate"]["userErrors"],
            json!([])
        );
        article_ids.push((
            title.to_string(),
            create.body["data"]["articleCreate"]["article"]["id"]
                .as_str()
                .unwrap()
                .to_string(),
        ));
    }

    let alpha_id = article_ids
        .iter()
        .find(|(title, _)| title.starts_with("Alpha"))
        .unwrap()
        .1
        .clone();
    let bravo_id = article_ids
        .iter()
        .find(|(title, _)| title.starts_with("Bravo"))
        .unwrap()
        .1
        .clone();
    let charlie_id = article_ids
        .iter()
        .find(|(title, _)| title.starts_with("Charlie"))
        .unwrap()
        .1
        .clone();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query NestedBlogArticles($blogId: ID!, $after: String!, $before: String!) {
          firstPage: blog(id: $blogId) {
            articlesFirst: articles(first: 2) {
              nodes { id title }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          afterPage: blog(id: $blogId) {
            articlesAfter: articles(first: 2, after: $after) {
              nodes { id title }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          beforePage: blog(id: $blogId) {
            articlesBefore: articles(last: 1, before: $before) {
              nodes { id title }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          reversePage: blog(id: $blogId) {
            articlesReverse: articles(first: 2, reverse: true) {
              nodes { id title }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({"blogId": blog_id, "after": bravo_id, "before": charlie_id}),
    ));

    assert_eq!(
        read.body["data"]["firstPage"]["articlesFirst"]["nodes"],
        json!([
            {"id": alpha_id, "title": "Alpha nested article"},
            {"id": bravo_id, "title": "Bravo nested article"}
        ])
    );
    assert_eq!(
        read.body["data"]["firstPage"]["articlesFirst"]["edges"],
        json!([
            {"cursor": alpha_id, "node": {"id": alpha_id}},
            {"cursor": bravo_id, "node": {"id": bravo_id}}
        ])
    );
    assert_eq!(
        read.body["data"]["firstPage"]["articlesFirst"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": alpha_id,
            "endCursor": bravo_id
        })
    );
    assert_eq!(
        read.body["data"]["afterPage"]["articlesAfter"]["nodes"],
        json!([{"id": charlie_id, "title": "Charlie nested article"}])
    );
    assert_eq!(
        read.body["data"]["afterPage"]["articlesAfter"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );
    assert_eq!(
        read.body["data"]["beforePage"]["articlesBefore"]["nodes"],
        json!([{"id": bravo_id, "title": "Bravo nested article"}])
    );
    assert_eq!(
        read.body["data"]["beforePage"]["articlesBefore"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(
        read.body["data"]["reversePage"]["articlesReverse"]["nodes"],
        json!([
            {"id": charlie_id, "title": "Charlie nested article"},
            {"id": bravo_id, "title": "Bravo nested article"}
        ])
    );
}

#[test]
fn online_store_content_back_references_project_full_parent_records() {
    let comment_id = "gid://shopify/Comment/9203";
    let hydrated_article_id = Arc::new(Mutex::new(String::new()));
    let upstream_calls = Arc::new(Mutex::new(0_usize));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let hydrated_article_id = Arc::clone(&hydrated_article_id);
        let upstream_calls = Arc::clone(&upstream_calls);
        move |request| {
            *upstream_calls.lock().unwrap() += 1;
            let body: Value =
                serde_json::from_str(&request.body).expect("upstream GraphQL body parses");
            let query = body["query"].as_str().unwrap_or_default();
            assert!(
                query.contains("OnlineStoreCommentHydrate"),
                "unexpected online-store hydrate query: {query}"
            );
            let article_id = hydrated_article_id.lock().unwrap().clone();
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "comment": {
                            "__typename": "Comment",
                            "id": comment_id,
                            "status": "UNAPPROVED",
                            "body": "Back reference comment",
                            "bodyHtml": "<p>Back reference comment</p>",
                            "isPublished": false,
                            "publishedAt": null,
                            "createdAt": "2026-01-01T00:00:00Z",
                            "updatedAt": "2026-01-01T00:00:00Z",
                            "article": { "id": article_id }
                        }
                    }
                }),
            }
        }
    });

    let blog_create = proxy.process_request(json_graphql_request(
        r#"
        mutation BackReferenceBlog($blog: BlogCreateInput!) {
          blogCreate(blog: $blog) {
            blog { id title handle commentPolicy createdAt updatedAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({"blog": {"title": "Back Reference Blog", "commentPolicy": "MODERATED"}}),
    ));
    assert_eq!(
        blog_create.body["data"]["blogCreate"]["userErrors"],
        json!([])
    );
    let blog = &blog_create.body["data"]["blogCreate"]["blog"];
    let blog_id = blog["id"].as_str().unwrap().to_string();
    let blog_created_at = blog["createdAt"].clone();
    let blog_updated_at = blog["updatedAt"].clone();

    let article_create = proxy.process_request(json_graphql_request(
        r#"
        mutation BackReferenceArticle($article: ArticleCreateInput!) {
          articleCreate(article: $article) {
            article {
              id
              title
              publishedAt
              author { name }
              blog { id title handle commentPolicy createdAt updatedAt }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"article": {
            "title": "Back Reference Article",
            "blogId": blog_id,
            "author": {"name": "Initial Back Reference Author"},
            "isPublished": true
        }}),
    ));
    assert_eq!(
        article_create.body["data"]["articleCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        article_create.body["data"]["articleCreate"]["article"]["blog"],
        json!({
            "id": blog_id,
            "title": "Back Reference Blog",
            "handle": "back-reference-blog",
            "commentPolicy": "MODERATED",
            "createdAt": blog_created_at,
            "updatedAt": blog_updated_at
        })
    );
    let article_id = article_create.body["data"]["articleCreate"]["article"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    *hydrated_article_id.lock().unwrap() = article_id.clone();

    let blog_update = proxy.process_request(json_graphql_request(
        r#"
        mutation BackReferenceBlogUpdate($id: ID!, $blog: BlogUpdateInput!) {
          blogUpdate(id: $id, blog: $blog) {
            blog { id commentPolicy updatedAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({"id": blog_id, "blog": {"commentPolicy": "CLOSED"}}),
    ));
    assert_eq!(
        blog_update.body["data"]["blogUpdate"]["userErrors"],
        json!([])
    );
    let blog_updated_again_at = blog_update.body["data"]["blogUpdate"]["blog"]["updatedAt"].clone();

    let article_after_blog_update = proxy.process_request(json_graphql_request(
        r#"
        query ArticleBackReferenceAfterBlogUpdate($id: ID!) {
          article(id: $id) {
            id
            blog { id title handle commentPolicy createdAt updatedAt }
          }
        }
        "#,
        json!({"id": article_id}),
    ));
    assert_eq!(
        article_after_blog_update.body["data"]["article"]["blog"],
        json!({
            "id": blog_id,
            "title": "Back Reference Blog",
            "handle": "back-reference-blog",
            "commentPolicy": "CLOSED",
            "createdAt": blog_created_at,
            "updatedAt": blog_updated_again_at
        })
    );

    let comment_approve = proxy.process_request(json_graphql_request(
        r#"
        mutation BackReferenceComment($id: ID!) {
          commentApprove(id: $id) {
            comment {
              id
              status
              article {
                id
                title
                publishedAt
                author { name }
                blog { id title handle commentPolicy createdAt updatedAt }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({"id": comment_id}),
    ));
    assert_eq!(
        comment_approve.body["data"]["commentApprove"]["userErrors"],
        json!([])
    );
    assert_eq!(
        comment_approve.body["data"]["commentApprove"]["comment"]["article"]["author"],
        json!({"name": "Initial Back Reference Author"})
    );
    assert_eq!(
        comment_approve.body["data"]["commentApprove"]["comment"]["article"]["blog"]
            ["commentPolicy"],
        json!("CLOSED")
    );

    let article_update = proxy.process_request(json_graphql_request(
        r#"
        mutation BackReferenceArticleUpdate($id: ID!, $article: ArticleUpdateInput!) {
          articleUpdate(id: $id, article: $article) {
            article { id title author { name } }
            userErrors { field message code }
          }
        }
        "#,
        json!({"id": article_id, "article": {
            "title": "Updated Back Reference Article",
            "author": {"name": "Updated Back Reference Author"}
        }}),
    ));
    assert_eq!(
        article_update.body["data"]["articleUpdate"]["userErrors"],
        json!([])
    );

    let comment_read = proxy.process_request(json_graphql_request(
        r#"
        query CommentBackReferenceAfterArticleUpdate($id: ID!) {
          comment(id: $id) {
            id
            article {
              id
              title
              author { name }
              publishedAt
              blog { id commentPolicy }
            }
          }
        }
        "#,
        json!({"id": comment_id}),
    ));
    assert_eq!(
        comment_read.body["data"]["comment"]["article"]["title"],
        json!("Updated Back Reference Article")
    );
    assert_eq!(
        comment_read.body["data"]["comment"]["article"]["author"],
        json!({"name": "Updated Back Reference Author"})
    );
    assert_eq!(
        comment_read.body["data"]["comment"]["article"]["blog"],
        json!({"id": blog_id, "commentPolicy": "CLOSED"})
    );
    assert_eq!(*upstream_calls.lock().unwrap(), 1);
}


#[test]
fn online_store_articles_published_status_query_controls_visibility() {
    let mut proxy = snapshot_proxy();

    let blog = proxy.process_request(json_graphql_request(
        r#"
        mutation ArticleStatusBlog {
          blogCreate(blog: { title: "Article status blog" }) {
            blog { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(blog.body["data"]["blogCreate"]["userErrors"], json!([]));
    let blog_id = blog.body["data"]["blogCreate"]["blog"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let articles = proxy.process_request(json_graphql_request(
        r#"
        mutation ArticleStatusSetup($blogId: ID!) {
          published: articleCreate(article: { title: "Published article", blogId: $blogId, author: { name: "Status Author" }, tags: ["status-tag"], isPublished: true }) {
            article { id title isPublished }
            userErrors { field message code }
          }
          draft: articleCreate(article: { title: "Draft article", blogId: $blogId, author: { name: "Status Author" }, tags: ["status-tag", "draft-tag"], isPublished: false }) {
            article { id title isPublished }
            userErrors { field message code }
          }
        }
        "#,
        json!({"blogId": blog_id}),
    ));
    assert_eq!(articles.body["data"]["published"]["userErrors"], json!([]));
    assert_eq!(articles.body["data"]["draft"]["userErrors"], json!([]));
    let published_id = articles.body["data"]["published"]["article"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let draft_id = articles.body["data"]["draft"]["article"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query ArticleStatusReads {
          defaultPublished: articles(first: 10) { nodes { id title isPublished } }
          explicitPublished: articles(first: 10, query: "published_status:published") { nodes { id title isPublished } }
          anyStatus: articles(first: 10, query: "published_status:any") { nodes { id title isPublished } }
          unpublishedOnly: articles(first: 10, query: "published_status:unpublished") { nodes { id title isPublished } }
          byAuthor: articles(first: 10, query: "published_status:published author:'Status Author'") { nodes { id title isPublished } }
          byTag: articles(first: 10, query: "published_status:published tag:status-tag") { nodes { id title isPublished } }
          byBlogTitle: articles(first: 10, query: "published_status:published blog_title:'Article status blog'") { nodes { id title isPublished } }
          draftTagDefault: articles(first: 10, query: "draft-tag") { nodes { id title isPublished } }
          draftTagAny: articles(first: 10, query: "published_status:any draft-tag") { nodes { id title isPublished } }
          allTags: articleTags(limit: 20)
          titleSorted: articles(first: 10, query: "published_status:any", sortKey: TITLE) { nodes { id title } }
          titleSortedReverse: articles(first: 10, query: "published_status:any", sortKey: TITLE, reverse: true) { nodes { id title } }
          unknownFilter: articles(first: 10, query: "not_a_real_filter:value") { nodes { id title } }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        read.body["data"]["defaultPublished"]["nodes"],
        json!([
            {"id": published_id, "title": "Published article", "isPublished": true},
            {"id": draft_id, "title": "Draft article", "isPublished": false}
        ])
    );
    assert_eq!(
        read.body["data"]["explicitPublished"]["nodes"],
        json!([{"id": published_id, "title": "Published article", "isPublished": true}])
    );
    assert_eq!(
        read.body["data"]["anyStatus"]["nodes"],
        json!([
            {"id": published_id, "title": "Published article", "isPublished": true},
            {"id": draft_id, "title": "Draft article", "isPublished": false}
        ])
    );
    assert_eq!(
        read.body["data"]["unpublishedOnly"]["nodes"],
        json!([{"id": draft_id, "title": "Draft article", "isPublished": false}])
    );
    let published_article =
        json!([{"id": published_id, "title": "Published article", "isPublished": true}]);
    assert_eq!(read.body["data"]["byAuthor"]["nodes"], published_article);
    assert_eq!(read.body["data"]["byTag"]["nodes"], published_article);
    assert_eq!(read.body["data"]["byBlogTitle"]["nodes"], published_article);
    assert_eq!(read.body["data"]["draftTagDefault"]["nodes"], json!([]));
    assert_eq!(
        read.body["data"]["draftTagAny"]["nodes"],
        json!([{"id": draft_id, "title": "Draft article", "isPublished": false}])
    );
    assert_eq!(
        read.body["data"]["allTags"],
        json!(["draft-tag", "status-tag"])
    );
    assert_eq!(
        read.body["data"]["titleSorted"]["nodes"],
        json!([
            {"id": draft_id, "title": "Draft article"},
            {"id": published_id, "title": "Published article"}
        ])
    );
    assert_eq!(
        read.body["data"]["titleSortedReverse"]["nodes"],
        json!([
            {"id": published_id, "title": "Published article"},
            {"id": draft_id, "title": "Draft article"}
        ])
    );
    assert_eq!(read.body["data"]["unknownFilter"]["nodes"], json!([]));
}

#[test]
fn online_store_article_metafields_reflect_staged_create_and_update() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation ArticleMetafieldsCreate($article: ArticleCreateInput!, $blog: ArticleBlogInput) {
          articleCreate(article: $article, blog: $blog) {
            article {
              id
              handle
              metafield(namespace: "online_store_conformance", key: "hero") {
                id
                namespace
                key
                type
                value
                jsonValue
                ownerType
              }
              metafields(first: 1, namespace: "online_store_conformance") {
                nodes { id namespace key type value jsonValue ownerType }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "blog": { "title": "Article metafield blog" },
            "article": {
                "title": "Article metafields",
                "author": { "name": "Metafield Author" },
                "metafields": [
                    {
                        "namespace": "online_store_conformance",
                        "key": "hero",
                        "type": "single_line_text_field",
                        "value": "created hero"
                    },
                    {
                        "namespace": "online_store_conformance",
                        "key": "secondary",
                        "type": "single_line_text_field",
                        "value": "created secondary"
                    },
                    {
                        "namespace": "other_namespace",
                        "key": "hidden",
                        "type": "single_line_text_field",
                        "value": "hidden"
                    }
                ]
            }
        }),
    ));
    assert_eq!(
        create.body["data"]["articleCreate"]["userErrors"],
        json!([])
    );
    let article = &create.body["data"]["articleCreate"]["article"];
    let article_id = article["id"].as_str().unwrap().to_string();
    let article_handle = article["handle"].as_str().unwrap().to_string();
    assert_eq!(
        article["metafield"],
        json!({
            "id": article["metafield"]["id"].clone(),
            "namespace": "online_store_conformance",
            "key": "hero",
            "type": "single_line_text_field",
            "value": "created hero",
            "jsonValue": "created hero",
            "ownerType": "ARTICLE"
        })
    );
    assert_eq!(article["metafields"]["nodes"][0]["key"], json!("hero"));
    assert_eq!(
        article["metafields"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(
        article["metafields"]["pageInfo"]["hasPreviousPage"],
        json!(false)
    );
    let after_cursor = article["metafields"]["pageInfo"]["endCursor"]
        .as_str()
        .unwrap()
        .to_string();

    let read_after = proxy.process_request(json_graphql_request(
        r#"
        query ArticleMetafieldsAfter($id: ID!, $after: String!) {
          article(id: $id) {
            metafields(first: 2, namespace: "online_store_conformance", after: $after) {
              nodes { key value }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({"id": article_id, "after": after_cursor}),
    ));
    assert_eq!(
        read_after.body["data"]["article"]["metafields"]["nodes"],
        json!([{ "key": "secondary", "value": "created secondary" }])
    );
    assert_eq!(
        read_after.body["data"]["article"]["metafields"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation ArticleMetafieldsUpdate($id: ID!, $article: ArticleUpdateInput!) {
          articleUpdate(id: $id, article: $article) {
            article {
              title
              handle
              metafield(namespace: "online_store_conformance", key: "hero") {
                id
                namespace
                key
                type
                value
                jsonValue
                ownerType
              }
              metafields(first: 5, namespace: "online_store_conformance") {
                nodes { key value jsonValue }
                pageInfo { hasNextPage hasPreviousPage }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "id": article_id,
            "article": {
                "title": "Article metafields renamed",
                "metafields": [{
                    "namespace": "online_store_conformance",
                    "key": "hero",
                    "type": "single_line_text_field",
                    "value": "updated hero"
                }]
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["articleUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["articleUpdate"]["article"]["title"],
        json!("Article metafields renamed")
    );
    assert_eq!(
        update.body["data"]["articleUpdate"]["article"]["handle"],
        json!(article_handle)
    );
    assert_eq!(
        update.body["data"]["articleUpdate"]["article"]["metafield"]["value"],
        json!("updated hero")
    );
    assert_eq!(
        update.body["data"]["articleUpdate"]["article"]["metafield"]["jsonValue"],
        json!("updated hero")
    );
    assert_eq!(
        update.body["data"]["articleUpdate"]["article"]["metafields"]["nodes"],
        json!([
            {
                "key": "hero",
                "value": "updated hero",
                "jsonValue": "updated hero"
            },
            {
                "key": "secondary",
                "value": "created secondary",
                "jsonValue": "created secondary"
            }
        ])
    );
}

#[test]
fn online_store_blog_and_page_connections_filter_sort_and_reverse() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation BlogPageConnectionSetup {
          zBlog: blogCreate(blog: { title: "Zulu content blog" }) {
            blog { id title }
            userErrors { field message code }
          }
          aBlog: blogCreate(blog: { title: "Alpha content blog" }) {
            blog { id title }
            userErrors { field message code }
          }
          zPage: pageCreate(page: { title: "Zulu content page", body: "<p>Zulu page body</p>", isPublished: true }) {
            page { id title isPublished }
            userErrors { field message code }
          }
          aPage: pageCreate(page: { title: "Alpha content page", body: "<p>Alpha page body</p>", isPublished: false }) {
            page { id title isPublished }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(create.body["data"]["zBlog"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["aBlog"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["zPage"]["userErrors"], json!([]));
    assert_eq!(create.body["data"]["aPage"]["userErrors"], json!([]));
    let z_blog = create.body["data"]["zBlog"]["blog"]["id"].clone();
    let a_blog = create.body["data"]["aBlog"]["blog"]["id"].clone();
    let z_page = create.body["data"]["zPage"]["page"]["id"].clone();
    let a_page = create.body["data"]["aPage"]["page"]["id"].clone();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query BlogPageConnectionFilters {
          blogsByTitle: blogs(first: 10, query: "title:'Alpha content blog'") { nodes { id title } }
          blogsSorted: blogs(first: 10, sortKey: TITLE) { nodes { id title } }
          blogsSortedReverse: blogs(first: 10, sortKey: TITLE, reverse: true) { nodes { id title } }
          blogsUnknownFilter: blogs(first: 10, query: "not_a_real_filter:value") { nodes { id title } }
          pagesByTitle: pages(first: 10, query: "title:'Alpha content page'") { nodes { id title isPublished } }
          pagesPublished: pages(first: 10, query: "published_status:published") { nodes { id title isPublished } }
          pagesUnpublished: pages(first: 10, query: "published_status:unpublished") { nodes { id title isPublished } }
          pagesSorted: pages(first: 10, sortKey: TITLE) { nodes { id title } }
          pagesSortedReverse: pages(first: 10, sortKey: TITLE, reverse: true) { nodes { id title } }
          pagesUnknownFilter: pages(first: 10, query: "not_a_real_filter:value") { nodes { id title } }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        read.body["data"]["blogsByTitle"]["nodes"],
        json!([{"id": a_blog, "title": "Alpha content blog"}])
    );
    assert_eq!(
        read.body["data"]["blogsSorted"]["nodes"],
        json!([
            {"id": a_blog, "title": "Alpha content blog"},
            {"id": z_blog, "title": "Zulu content blog"}
        ])
    );
    assert_eq!(
        read.body["data"]["blogsSortedReverse"]["nodes"],
        json!([
            {"id": z_blog, "title": "Zulu content blog"},
            {"id": a_blog, "title": "Alpha content blog"}
        ])
    );
    assert_eq!(read.body["data"]["blogsUnknownFilter"]["nodes"], json!([]));
    assert_eq!(
        read.body["data"]["pagesByTitle"]["nodes"],
        json!([{"id": a_page, "title": "Alpha content page", "isPublished": false}])
    );
    assert_eq!(
        read.body["data"]["pagesPublished"]["nodes"],
        json!([{"id": z_page, "title": "Zulu content page", "isPublished": true}])
    );
    assert_eq!(
        read.body["data"]["pagesUnpublished"]["nodes"],
        json!([{"id": a_page, "title": "Alpha content page", "isPublished": false}])
    );
    assert_eq!(
        read.body["data"]["pagesSorted"]["nodes"],
        json!([
            {"id": a_page, "title": "Alpha content page"},
            {"id": z_page, "title": "Zulu content page"}
        ])
    );
    assert_eq!(
        read.body["data"]["pagesSortedReverse"]["nodes"],
        json!([
            {"id": z_page, "title": "Zulu content page"},
            {"id": a_page, "title": "Alpha content page"}
        ])
    );
    assert_eq!(read.body["data"]["pagesUnknownFilter"]["nodes"], json!([]));
}

#[test]
fn online_store_comments_connection_filters_sort_and_reverse_hydrated_state() {
    let article_id = "gid://shopify/Article/9102";
    let alpha_comment_id = "gid://shopify/Comment/9103";
    let zulu_comment_id = "gid://shopify/Comment/9104";
    let upstream_calls = Arc::new(Mutex::new(0_usize));
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
            let upstream_calls = upstream_calls.clone();
            move |_request| {
                *upstream_calls.lock().unwrap() += 1;
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "comments": {
                                "nodes": [
                                    {
                                        "__typename": "Comment",
                                        "id": zulu_comment_id,
                                        "status": "SPAM",
                                        "body": "Zulu comment body",
                                        "bodyHtml": "<p>Zulu comment body</p>",
                                        "isPublished": false,
                                        "publishedAt": null,
                                        "createdAt": "2026-01-02T00:00:00Z",
                                        "updatedAt": "2026-01-02T00:00:00Z",
                                        "article": { "id": article_id, "title": "Hydrated comments article" }
                                    },
                                    {
                                        "__typename": "Comment",
                                        "id": alpha_comment_id,
                                        "status": "PUBLISHED",
                                        "body": "Alpha comment body",
                                        "bodyHtml": "<p>Alpha comment body</p>",
                                        "isPublished": true,
                                        "publishedAt": "2026-01-01T00:00:00Z",
                                        "createdAt": "2026-01-01T00:00:00Z",
                                        "updatedAt": "2026-01-01T00:00:00Z",
                                        "article": { "id": article_id, "title": "Hydrated comments article" }
                                    }
                                ],
                                "pageInfo": {
                                    "hasNextPage": false,
                                    "hasPreviousPage": false,
                                    "startCursor": "cursor-a",
                                    "endCursor": "cursor-z"
                                }
                            }
                        }
                    }),
                }
            }
        });

    let hydrate = proxy.process_request(json_graphql_request(
        r#"
        query HydrateComments {
          comments(first: 10) {
            nodes { id body status isPublished publishedAt createdAt updatedAt article { id title } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(hydrate.status, 200);
    assert_eq!(*upstream_calls.lock().unwrap(), 1);

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CommentConnectionFilters {
          bodyQuery: comments(first: 10, query: "body:'Alpha comment'") { nodes { id body status } }
          statusQuery: comments(first: 10, query: "status:SPAM") { nodes { id body status } }
          createdSorted: comments(first: 10, sortKey: CREATED_AT) { nodes { id body createdAt } }
          createdSortedReverse: comments(first: 10, sortKey: CREATED_AT, reverse: true) { nodes { id body createdAt } }
          unknownFilter: comments(first: 10, query: "not_a_real_filter:value") { nodes { id body } }
        }
        "#,
        json!({}),
    ));

    assert_eq!(
        read.body["data"]["bodyQuery"]["nodes"],
        json!([{"id": alpha_comment_id, "body": "Alpha comment body", "status": "PUBLISHED"}])
    );
    assert_eq!(
        read.body["data"]["statusQuery"]["nodes"],
        json!([{"id": zulu_comment_id, "body": "Zulu comment body", "status": "SPAM"}])
    );
    assert_eq!(
        read.body["data"]["createdSorted"]["nodes"],
        json!([
            {"id": alpha_comment_id, "body": "Alpha comment body", "createdAt": "2026-01-01T00:00:00Z"},
            {"id": zulu_comment_id, "body": "Zulu comment body", "createdAt": "2026-01-02T00:00:00Z"}
        ])
    );
    assert_eq!(
        read.body["data"]["createdSortedReverse"]["nodes"],
        json!([
            {"id": zulu_comment_id, "body": "Zulu comment body", "createdAt": "2026-01-02T00:00:00Z"},
            {"id": alpha_comment_id, "body": "Alpha comment body", "createdAt": "2026-01-01T00:00:00Z"}
        ])
    );
    assert_eq!(read.body["data"]["unknownFilter"]["nodes"], json!([]));
    assert_eq!(*upstream_calls.lock().unwrap(), 1);
}

#[test]
fn online_store_nested_article_comments_connection_honors_args_and_staged_moderation() {
    let blog_id = "gid://shopify/Blog/9201";
    let article_id = "gid://shopify/Article/9202";
    let alpha_comment_id = "gid://shopify/Comment/9203";
    let bravo_comment_id = "gid://shopify/Comment/9204";
    let charlie_comment_id = "gid://shopify/Comment/9205";
    let upstream_calls = Arc::new(Mutex::new(0_usize));
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
            let upstream_calls = upstream_calls.clone();
            move |_request| {
                *upstream_calls.lock().unwrap() += 1;
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "article": {
                                "__typename": "Article",
                                "id": article_id,
                                "title": "Nested comments article",
                                "handle": "nested-comments-article",
                                "createdAt": "2026-01-01T00:00:00Z",
                                "updatedAt": "2026-01-01T00:00:00Z",
                                "blog": {
                                    "id": blog_id,
                                    "title": "Nested comments blog",
                                    "handle": "nested-comments-blog"
                                },
                                "comments": {
                                    "nodes": [
                                        {
                                            "__typename": "Comment",
                                            "id": alpha_comment_id,
                                            "status": "PUBLISHED",
                                            "body": "Alpha comment body",
                                            "bodyHtml": "<p>Alpha comment body</p>",
                                            "isPublished": true,
                                            "publishedAt": "2026-01-01T00:00:00Z",
                                            "createdAt": "2026-01-01T00:00:00Z",
                                            "updatedAt": "2026-01-01T00:00:00Z",
                                            "article": { "id": article_id, "title": "Nested comments article" }
                                        },
                                        {
                                            "__typename": "Comment",
                                            "id": bravo_comment_id,
                                            "status": "UNAPPROVED",
                                            "body": "Bravo comment body",
                                            "bodyHtml": "<p>Bravo comment body</p>",
                                            "isPublished": false,
                                            "publishedAt": null,
                                            "createdAt": "2026-01-02T00:00:00Z",
                                            "updatedAt": "2026-01-02T00:00:00Z",
                                            "article": { "id": article_id, "title": "Nested comments article" }
                                        },
                                        {
                                            "__typename": "Comment",
                                            "id": charlie_comment_id,
                                            "status": "PUBLISHED",
                                            "body": "Charlie comment body",
                                            "bodyHtml": "<p>Charlie comment body</p>",
                                            "isPublished": true,
                                            "publishedAt": "2026-01-03T00:00:00Z",
                                            "createdAt": "2026-01-03T00:00:00Z",
                                            "updatedAt": "2026-01-03T00:00:00Z",
                                            "article": { "id": article_id, "title": "Nested comments article" }
                                        }
                                    ],
                                    "pageInfo": {
                                        "hasNextPage": false,
                                        "hasPreviousPage": false,
                                        "startCursor": "upstream-alpha",
                                        "endCursor": "upstream-charlie"
                                    }
                                }
                            }
                        }
                    }),
                }
            }
        });

    let hydrate = proxy.process_request(json_graphql_request(
        r#"
        query HydrateNestedComments($articleId: ID!) {
          article(id: $articleId) {
            id
            title
            handle
            createdAt
            updatedAt
            blog { id title handle }
            comments(first: 10) {
              nodes {
                id
                status
                body
                bodyHtml
                isPublished
                publishedAt
                createdAt
                updatedAt
                article { id title }
              }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({"articleId": article_id}),
    ));
    assert_eq!(hydrate.status, 200);
    assert_eq!(*upstream_calls.lock().unwrap(), 1);

    let spam = proxy.process_request(json_graphql_request(
        r#"
        mutation StageCommentModeration($id: ID!) {
          commentSpam(id: $id) {
            comment { id status isPublished publishedAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({"id": alpha_comment_id}),
    ));
    assert_eq!(spam.body["data"]["commentSpam"]["userErrors"], json!([]));
    assert_eq!(
        spam.body["data"]["commentSpam"]["comment"],
        json!({
            "id": alpha_comment_id,
            "status": "SPAM",
            "isPublished": false,
            "publishedAt": null
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query NestedArticleComments($articleId: ID!, $after: String!, $before: String!) {
          firstPage: article(id: $articleId) {
            commentsFirst: comments(first: 2) {
              nodes { id status body }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          afterPage: article(id: $articleId) {
            commentsAfter: comments(first: 2, after: $after) {
              nodes { id status }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          beforePage: article(id: $articleId) {
            commentsBefore: comments(last: 1, before: $before) {
              nodes { id status }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          reversePage: article(id: $articleId) {
            commentsReverse: comments(first: 1, reverse: true) {
              nodes { id status }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          spamOnly: article(id: $articleId) {
            commentsSpam: comments(first: 5, query: "status:SPAM") {
              nodes { id status }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({
            "articleId": article_id,
            "after": bravo_comment_id,
            "before": charlie_comment_id
        }),
    ));

    assert_eq!(
        read.body["data"]["firstPage"]["commentsFirst"]["nodes"],
        json!([
            {"id": alpha_comment_id, "status": "SPAM", "body": "Alpha comment body"},
            {"id": bravo_comment_id, "status": "UNAPPROVED", "body": "Bravo comment body"}
        ])
    );
    assert_eq!(
        read.body["data"]["firstPage"]["commentsFirst"]["edges"],
        json!([
            {"cursor": alpha_comment_id, "node": {"id": alpha_comment_id}},
            {"cursor": bravo_comment_id, "node": {"id": bravo_comment_id}}
        ])
    );
    assert_eq!(
        read.body["data"]["firstPage"]["commentsFirst"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": alpha_comment_id,
            "endCursor": bravo_comment_id
        })
    );
    assert_eq!(
        read.body["data"]["afterPage"]["commentsAfter"]["nodes"],
        json!([{"id": charlie_comment_id, "status": "PUBLISHED"}])
    );
    assert_eq!(
        read.body["data"]["afterPage"]["commentsAfter"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );
    assert_eq!(
        read.body["data"]["beforePage"]["commentsBefore"]["nodes"],
        json!([{"id": bravo_comment_id, "status": "UNAPPROVED"}])
    );
    assert_eq!(
        read.body["data"]["beforePage"]["commentsBefore"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(
        read.body["data"]["reversePage"]["commentsReverse"]["nodes"],
        json!([{"id": charlie_comment_id, "status": "PUBLISHED"}])
    );
    assert_eq!(
        read.body["data"]["spamOnly"]["commentsSpam"]["nodes"],
        json!([{"id": alpha_comment_id, "status": "SPAM"}])
    );
    assert_eq!(*upstream_calls.lock().unwrap(), 1);
}

#[test]
fn online_store_page_create_defaults_to_published_and_reads_back() {
    let mut proxy = snapshot_proxy();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation PageDefaultPublishLocalStaging {
          pageCreate(page: { title: "Default Published Page", body: "<p>Visible <strong>body</strong></p>" }) {
            page { id title handle bodySummary isPublished publishedAt }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(create.body["data"]["pageCreate"]["userErrors"], json!([]));
    let page_id = create.body["data"]["pageCreate"]["page"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let published_at = assert_online_store_operation_timestamp(
        &create.body["data"]["pageCreate"]["page"]["publishedAt"],
        "pageCreate.publishedAt",
    );
    assert_eq!(
        create.body["data"]["pageCreate"]["page"],
        json!({
            "id": page_id,
            "title": "Default Published Page",
            "handle": "default-published-page",
            "bodySummary": "Visible body",
            "isPublished": true,
            "publishedAt": published_at.clone()
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query PageDefaultPublishRead($id: ID!) {
          page(id: $id) { id title handle isPublished publishedAt }
          pages(first: 10) { nodes { id title handle isPublished publishedAt } }
          pagesCount { count precision }
        }
        "#,
        json!({"id": page_id}),
    ));
    assert_eq!(
        read.body["data"]["page"],
        json!({
            "id": page_id,
            "title": "Default Published Page",
            "handle": "default-published-page",
            "isPublished": true,
            "publishedAt": published_at.clone()
        })
    );
    assert_eq!(
        read.body["data"]["pages"]["nodes"],
        json!([{
            "id": page_id,
            "title": "Default Published Page",
            "handle": "default-published-page",
            "isPublished": true,
            "publishedAt": published_at
        }])
    );
    assert_eq!(
        read.body["data"]["pagesCount"],
        json!({"count": 1, "precision": "EXACT"})
    );
    assert_eq!(log_snapshot(&proxy)["entries"].as_array().unwrap().len(), 1);
}

#[test]
fn online_store_comment_moderation_state_machine_and_delete_are_local() {
    let blog_id = "gid://shopify/Blog/9001";
    let article_id = "gid://shopify/Article/9002";
    let unapproved_id = "gid://shopify/Comment/9003";
    let spam_id = "gid://shopify/Comment/9004";
    let published_id = "gid://shopify/Comment/9005";
    let still_unapproved_id = "gid://shopify/Comment/9006";
    let hydrate_calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let transport_hydrate_calls = Arc::clone(&hydrate_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value =
                serde_json::from_str(&request.body).expect("upstream GraphQL body parses");
            let query = body["query"].as_str().unwrap_or_default();
            let id = body["variables"]["id"].as_str().unwrap_or_default();
            transport_hydrate_calls
                .lock()
                .unwrap()
                .push(format!("{id}:{query}"));
            let comment_node = |id: &str, status: &str, is_published: bool, published_at: Value| {
                json!({
                    "__typename": "Comment",
                    "id": id,
                    "status": status,
                    "body": "Moderation body",
                    "bodyHtml": "<p>Moderation body</p>",
                    "isPublished": is_published,
                    "publishedAt": published_at,
                    "createdAt": "2026-01-01T00:00:00Z",
                    "updatedAt": "2026-01-01T00:00:00Z",
                    "article": { "id": article_id }
                })
            };
            let response = if query.contains("OnlineStoreCommentHydrate") {
                let comment = match id {
                    id if id == unapproved_id => {
                        comment_node(id, "UNAPPROVED", false, Value::Null)
                    }
                    id if id == spam_id => comment_node(id, "SPAM", false, Value::Null),
                    id if id == published_id => {
                        comment_node(id, "PUBLISHED", true, json!("2026-01-01T00:00:00Z"))
                    }
                    id if id == still_unapproved_id => {
                        comment_node(id, "UNAPPROVED", false, Value::Null)
                    }
                    _ => Value::Null,
                };
                json!({ "comment": comment })
            } else if query.contains("OnlineStoreArticleDeleteCascadeHydrate") {
                json!({
                    "article": {
                        "__typename": "Article",
                        "id": article_id,
                        "title": "Moderated Article",
                        "handle": "moderated-article",
                        "createdAt": "2026-01-01T00:00:00Z",
                        "updatedAt": "2026-01-01T00:00:00Z",
                        "blog": { "id": blog_id },
                        "comments": { "nodes": [
                            comment_node(unapproved_id, "UNAPPROVED", false, Value::Null),
                            comment_node(spam_id, "SPAM", false, Value::Null),
                            comment_node(published_id, "PUBLISHED", true, json!("2026-01-01T00:00:00Z")),
                            comment_node(still_unapproved_id, "UNAPPROVED", false, Value::Null)
                        ] }
                    }
                })
            } else {
                panic!("unexpected online-store hydrate query: {query}");
            };
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": response }),
            }
        });

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownComment($id: ID!) {
          commentApprove(id: $id) { comment { id } userErrors { field message code } }
          commentSpam(id: $id) { comment { id } userErrors { field message code } }
          commentNotSpam(id: $id) { comment { id } userErrors { field message code } }
          commentDelete(id: $id) { deletedCommentId userErrors { field message code } }
        }
        "#,
        json!({"id": "gid://shopify/Comment/9999999999"}),
    ));
    assert_eq!(
        unknown.body["data"]["commentApprove"]["userErrors"],
        json!([{"field": ["id"], "message": "Comment does not exist", "code": "NOT_FOUND"}])
    );
    assert_eq!(
        unknown.body["data"]["commentDelete"]["userErrors"],
        json!([{"field": ["id"], "message": "Comment does not exist", "code": "NOT_FOUND"}])
    );

    let transitions = proxy.process_request(json_graphql_request(
        r#"
        mutation CommentTransitions($unapproved: ID!, $stillUnapproved: ID!, $spam: ID!, $published: ID!) {
          approve: commentApprove(id: $unapproved) { comment { id status isPublished publishedAt createdAt updatedAt } userErrors { field message code } }
          approveSpam: commentApprove(id: $spam) { comment { id status } userErrors { field message code } }
          notSpamUnapproved: commentNotSpam(id: $stillUnapproved) { comment { id status } userErrors { field message code } }
          spamPublished: commentSpam(id: $published) { comment { id status isPublished publishedAt createdAt updatedAt } userErrors { field message code } }
        }
        "#,
        json!({"unapproved": unapproved_id, "stillUnapproved": still_unapproved_id, "spam": spam_id, "published": published_id}),
    ));
    assert_eq!(
        transitions.body["data"]["approve"]["comment"]["status"],
        json!("PUBLISHED")
    );
    assert_eq!(
        transitions.body["data"]["approve"]["comment"]["createdAt"],
        json!("2026-01-01T00:00:00Z")
    );
    let approved_published_at = assert_online_store_operation_timestamp(
        &transitions.body["data"]["approve"]["comment"]["publishedAt"],
        "commentApprove.publishedAt",
    );
    let approved_updated_at = assert_online_store_operation_timestamp(
        &transitions.body["data"]["approve"]["comment"]["updatedAt"],
        "commentApprove.updatedAt",
    );
    assert_eq!(approved_published_at, approved_updated_at);
    assert_eq!(transitions.body["data"]["approve"]["userErrors"], json!([]));
    assert_eq!(
        transitions.body["data"]["approveSpam"]["userErrors"],
        json!([{"field": ["id"], "message": "Status cannot transition via \"approve\"", "code": null}])
    );
    assert_eq!(
        transitions.body["data"]["notSpamUnapproved"]["comment"],
        Value::Null
    );
    assert_eq!(
        transitions.body["data"]["spamPublished"]["comment"]["status"],
        json!("SPAM")
    );
    assert_eq!(
        transitions.body["data"]["spamPublished"]["comment"]["createdAt"],
        json!("2026-01-01T00:00:00Z")
    );
    assert_eq!(
        transitions.body["data"]["spamPublished"]["comment"]["publishedAt"],
        Value::Null
    );
    assert_online_store_operation_timestamp(
        &transitions.body["data"]["spamPublished"]["comment"]["updatedAt"],
        "commentSpam.updatedAt",
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation DeleteComment($id: ID!) {
          commentDelete(id: $id) { deletedCommentId userErrors { field message code } }
        }
        "#,
        json!({"id": published_id}),
    ));
    assert_eq!(
        delete.body["data"]["commentDelete"],
        json!({"deletedCommentId": published_id, "userErrors": []})
    );
    let read_after_delete = proxy.process_request(json_graphql_request(
        r#"
        query ReadAfterCommentDelete($commentId: ID!, $articleId: ID!) {
          comment(id: $commentId) { id status }
          article(id: $articleId) {
            commentsCount { count precision }
            comments(first: 10) { nodes { id } }
          }
          comments(first: 10) { nodes { id } }
        }
        "#,
        json!({"commentId": published_id, "articleId": article_id}),
    ));
    assert_eq!(read_after_delete.body["data"]["comment"], Value::Null);
    assert_eq!(
        read_after_delete.body["data"]["article"]["comments"]["nodes"],
        json!([
            {"id": unapproved_id},
            {"id": spam_id},
            {"id": still_unapproved_id}
        ])
    );
    assert_eq!(
        read_after_delete.body["data"]["article"]["commentsCount"],
        json!({"count": 3, "precision": "EXACT"})
    );
    let hydrate_calls_before_deleted_moderation = {
        let calls = hydrate_calls.lock().unwrap();
        calls.len()
    };
    let log_len_before_deleted_moderation =
        log_snapshot(&proxy)["entries"].as_array().unwrap().len();
    let deleted_moderation = proxy.process_request(json_graphql_request(
        r#"
        mutation ModerateDeletedComment($id: ID!) {
          approve: commentApprove(id: $id) { comment { id } userErrors { field message code } }
          spam: commentSpam(id: $id) { comment { id } userErrors { field message code } }
          notSpam: commentNotSpam(id: $id) { comment { id } userErrors { field message code } }
          repeatDelete: commentDelete(id: $id) { deletedCommentId userErrors { field message code } }
        }
        "#,
        json!({"id": published_id}),
    ));
    let deleted_comment_error =
        json!([{"field": ["id"], "message": "Comment does not exist", "code": "NOT_FOUND"}]);
    assert_eq!(
        deleted_moderation.body["data"]["approve"],
        json!({"comment": null, "userErrors": deleted_comment_error})
    );
    assert_eq!(
        deleted_moderation.body["data"]["spam"],
        json!({"comment": null, "userErrors": deleted_comment_error})
    );
    assert_eq!(
        deleted_moderation.body["data"]["notSpam"],
        json!({"comment": null, "userErrors": deleted_comment_error})
    );
    assert_eq!(
        deleted_moderation.body["data"]["repeatDelete"],
        json!({"deletedCommentId": null, "userErrors": deleted_comment_error})
    );
    assert_eq!(
        log_snapshot(&proxy)["entries"].as_array().unwrap().len(),
        log_len_before_deleted_moderation
    );
    assert_eq!(
        hydrate_calls.lock().unwrap().len(),
        hydrate_calls_before_deleted_moderation
    );
    let hydrate_calls = hydrate_calls.lock().unwrap();
    assert!(hydrate_calls
        .iter()
        .any(|call| call.contains("OnlineStoreCommentHydrate")));
    assert!(hydrate_calls
        .iter()
        .any(|call| call.contains("OnlineStoreArticleDeleteCascadeHydrate")));
}

#[test]
fn online_store_content_validation_branches_do_not_stage() {
    let mut proxy = snapshot_proxy();

    let page_handles = proxy.process_request(json_graphql_request(
        r#"
        mutation PageHandles($first: PageCreateInput!, $second: PageCreateInput!, $duplicate: PageCreateInput!) {
          first: pageCreate(page: $first) { page { id handle } userErrors { field message code } }
          second: pageCreate(page: $second) { page { id handle } userErrors { field message code } }
          duplicate: pageCreate(page: $duplicate) { page { id handle } userErrors { field message code } }
        }
        "#,
        json!({
            "first": {"title": "About Us"},
            "second": {"title": "About Us"},
            "duplicate": {"title": "Explicit Duplicate", "handle": "about-us"}
        }),
    ));
    assert_eq!(
        page_handles.body["data"]["first"]["page"]["handle"],
        json!("about-us")
    );
    assert_eq!(
        page_handles.body["data"]["second"]["page"]["handle"],
        json!("about-us-1")
    );
    assert_eq!(
        page_handles.body["data"]["duplicate"],
        json!({"page": null, "userErrors": [{"field": ["page", "handle"], "message": "Handle has already been taken", "code": "TAKEN"}]})
    );

    let invalid = proxy.process_request(json_graphql_request(
        r#"
        mutation InvalidContent($futurePage: PageCreateInput!, $longBlog: BlogCreateInput!, $badArticle: ArticleCreateInput!) {
          futurePage: pageCreate(page: $futurePage) { page { id } userErrors { field message code } }
          longBlog: blogCreate(blog: $longBlog) { blog { id } userErrors { field message code } }
          badArticle: articleCreate(article: $badArticle) { article { id } userErrors { field message code } }
        }
        "#,
        json!({
            "futurePage": {"title": "Future page", "isPublished": true, "publishDate": "2099-01-01T00:00:00Z"},
            "longBlog": {"title": "x".repeat(256)},
            "badArticle": {"title": "No blog", "author": {"name": "Author"}}
        }),
    ));
    assert_eq!(
        invalid.body["data"]["futurePage"],
        json!({"page": null, "userErrors": [{"field": ["page"], "message": "Can\u{2019}t set isPublished to true and also set a future publish date.", "code": "INVALID_PUBLISH_DATE"}]})
    );
    assert_eq!(
        invalid.body["data"]["longBlog"],
        json!({"blog": null, "userErrors": [{"field": ["blog", "title"], "message": "Title is too long (maximum is 255 characters)", "code": "TOO_LONG"}]})
    );
    assert_eq!(
        invalid.body["data"]["badArticle"],
        json!({"article": null, "userErrors": [{"field": ["article"], "message": "Must reference or create a blog when creating an article.", "code": "BLOG_REFERENCE_REQUIRED"}]})
    );

    let blog = proxy.process_request(json_graphql_request(
        r#"
        mutation BlogCommentableCreate($blog: BlogCreateInput!) {
          create: blogCreate(blog: $blog) { blog { id title commentPolicy } userErrors { field message code } }
        }
        "#,
        json!({"blog": {"title": "Commentable", "commentPolicy": "CLOSED"}}),
    ));
    let blog_id = blog.body["data"]["create"]["blog"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let commentable = proxy.process_request(json_graphql_request(
        r#"
        mutation BlogCommentable($id: ID!, $valid: BlogUpdateInput!, $invalid: BlogUpdateInput!) {
          valid: blogUpdate(id: $id, blog: $valid) { blog { id title commentPolicy } userErrors { field message code } }
          invalid: blogUpdate(id: $id, blog: $invalid) { blog { id title commentPolicy } userErrors { field message code } }
        }
        "#,
        json!({
            "id": blog_id,
            "valid": {"commentable": "MODERATE"},
            "invalid": {"commentable": "INVALID_VALUE"}
        }),
    ));
    assert_eq!(
        commentable.body["data"]["valid"]["blog"]["commentPolicy"],
        json!("MODERATED")
    );
    assert_eq!(
        commentable.body["data"]["invalid"],
        json!({"blog": null, "userErrors": [{"field": ["blog", "commentable"], "message": "Commentable is not included in the list", "code": "INCLUSION"}]})
    );
    let commentable_read = proxy.process_request(json_graphql_request(
        r#"
        query BlogCommentableRead($id: ID!) {
          blog(id: $id) { id title commentPolicy }
        }
        "#,
        json!({"id": blog_id}),
    ));
    assert_eq!(
        commentable_read.body["data"]["blog"]["commentPolicy"],
        json!("MODERATED")
    );
}
