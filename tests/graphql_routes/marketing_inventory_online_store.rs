use super::common::*;
use pretty_assertions::assert_eq;

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
        json!({"activityId": activity_id}),
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
    assert_eq!(
        omitted_url.body["data"]["changed"],
        json!({"marketingActivity": null, "userErrors": [{"field": ["input"], "message": "URL parameter value cannot be modified.", "code": "IMMUTABLE_URL_PARAMETER"}]})
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
    assert_eq!(
        read.body["data"]["marketingActivities"]["nodes"][0],
        json!({"title": "Child", "remoteId": "guard-child", "parentRemoteId": "guard-parent", "hierarchyLevel": "AD", "urlParameterValue": "guard-child-url", "utmParameters": {"campaign": "guard-child", "source": "email", "medium": "newsletter"}})
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
            "activityInput": {"id": "gid://shopify/MarketingActivity/native-no-event", "title": "Native no event", "status": "ACTIVE"},
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
        json!({"result": null, "userErrors": [{"field": ["channelHandle"], "message": "The channel handle is not recognized. Please contact your partner manager for more information.", "code": "INVALID_CHANNEL_HANDLE"}]})
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
        json!({"result": null, "userErrors": [{"field": ["channelHandle"], "message": "The channel handle is not recognized. Please contact your partner manager for more information.", "code": "INVALID_CHANNEL_HANDLE"}]})
    );
}

#[test]
fn marketing_native_activity_lifecycle_stages_update_and_invalid_extension_error() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation MarketingNativeActivityLifecycle($createInput: MarketingActivityCreateInput!, $updateInput: MarketingActivityUpdateInput!, $invalidExtensionInput: MarketingActivityCreateInput!) {
          createNative: marketingActivityCreate(input: $createInput) { userErrors { field message } }
          updateNative: marketingActivityUpdate(input: $updateInput) { marketingActivity { id title status statusLabel isExternal inMainWorkflowVersion marketingEvent { id } } redirectPath userErrors { field message } }
          invalidExtension: marketingActivityCreate(input: $invalidExtensionInput) { userErrors { field message } }
        }
        "#,
        json!({
            "createInput": {"marketingActivityExtensionId": "gid://shopify/MarketingActivityExtension/har-373-local-extension", "status": "DRAFT"},
            "updateInput": {"id": "gid://shopify/MarketingActivity/1", "title": "HAR-373 Native Activity Active", "status": "ACTIVE"},
            "invalidExtensionInput": {"marketingActivityExtensionId": "gid://shopify/MarketingActivityExtension/00000000-0000-0000-0000-000000000000", "status": "DRAFT"}
        }),
    ));
    assert_eq!(
        response.body["data"]["createNative"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["updateNative"]["marketingActivity"],
        json!({"id": "gid://shopify/MarketingActivity/1", "title": "HAR-373 Native Activity Active", "status": "ACTIVE", "statusLabel": "Sending", "isExternal": false, "inMainWorkflowVersion": true, "marketingEvent": null})
    );
    assert_eq!(
        response.body["data"]["invalidExtension"]["userErrors"],
        json!([{ "field": ["input", "marketingActivityExtensionId"], "message": "Could not find the marketing extension" }])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query MarketingNativeActivityRead($activityId: ID!) { marketingActivity(id: $activityId) { id title status statusLabel isExternal inMainWorkflowVersion marketingEvent { id } } }
        "#,
        json!({"activityId": "gid://shopify/MarketingActivity/1"}),
    ));
    assert_eq!(
        read.body["data"]["marketingActivity"],
        json!({"id": "gid://shopify/MarketingActivity/1", "title": "HAR-373 Native Activity Active", "status": "ACTIVE", "statusLabel": "Sending", "isExternal": false, "inMainWorkflowVersion": true, "marketingEvent": null})
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
            userErrors { field message code }
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
    let mut proxy = snapshot_proxy();

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
            {"inventoryItemId": "gid://shopify/InventoryItem/53204673823026", "locationId": "gid://shopify/Location/106318430514", "quantity": 7},
            {"inventoryItemId": "gid://shopify/InventoryItem/53204673823026", "locationId": "gid://shopify/Location/106318463282", "quantity": 2}
        ]}}),
    ));
    assert_eq!(
        set.body["data"]["inventorySetQuantities"]["userErrors"],
        json!([])
    );
    assert_eq!(
        set.body["data"]["inventorySetQuantities"]["inventoryAdjustmentGroup"]["changes"][0],
        json!({"name": "available", "delta": 7, "quantityAfterChange": 7, "ledgerDocumentUri": null, "location": {"id": "gid://shopify/Location/106318430514", "name": "Shop location"}})
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
        json!({"inventoryItemId": "gid://shopify/InventoryItem/53204673823026", "productId": "gid://shopify/Product/10171266400562"}),
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
        json!("2024-01-01T00:00:00.000Z")
    );
    assert_eq!(
        read_after_set.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [1]["updatedAt"],
        json!("2024-01-01T00:00:00.000Z")
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
            inventoryAdjustmentGroup { reason referenceDocumentUri changes { name delta quantityAfterChange ledgerDocumentUri location { id name } } }
            userErrors { field message }
          }
        }
        "#,
        json!({"input": {"reason": "correction", "referenceDocumentUri": "logistics://har-305/move/1777251367654", "changes": [{"inventoryItemId": "gid://shopify/InventoryItem/53204673823026", "quantity": 3, "from": {"locationId": "gid://shopify/Location/106318430514", "name": "available"}, "to": {"locationId": "gid://shopify/Location/106318430514", "name": "damaged", "ledgerDocumentUri": "ledger://har-305/move/to/1777251367654"}}]}}),
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
    assert_eq!(
        move_response.body["data"]["inventoryMoveQuantities"]["inventoryAdjustmentGroup"]
            ["changes"][0]["quantityAfterChange"],
        json!(4)
    );
    assert_eq!(
        move_response.body["data"]["inventoryMoveQuantities"]["inventoryAdjustmentGroup"]
            ["changes"][1]["delta"],
        json!(3)
    );
    assert_eq!(
        move_response.body["data"]["inventoryMoveQuantities"]["inventoryAdjustmentGroup"]
            ["changes"][1]["quantityAfterChange"],
        json!(3)
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
        json!({"inventoryItemId": "gid://shopify/InventoryItem/53204673823026", "productId": "gid://shopify/Product/10171266400562"}),
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
        json!("2024-01-01T00:00:01.000Z")
    );
    assert_eq!(
        read_after_move.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [1]["updatedAt"],
        json!("2024-01-01T00:00:00.000Z")
    );
    assert_eq!(
        read_after_move.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [2]["quantity"],
        json!(3)
    );
    assert_eq!(
        read_after_move.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"]
            [2]["updatedAt"],
        json!("2024-01-01T00:00:01.000Z")
    );

    let blocked_set = proxy.process_request(json_graphql_request(
        r#"
        mutation InventoryQuantitySet($input: InventorySetQuantitiesInput!, $idempotencyKey: String!) {
          inventorySetQuantities(input: $input) @idempotent(key: $idempotencyKey) {
            userErrors { field message }
          }
        }
        "#,
        json!({"idempotencyKey": "inventory-set-missing-change-from", "input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://har-305/set/blocked", "quantities": [{"inventoryItemId": "gid://shopify/InventoryItem/53204673823026", "locationId": "gid://shopify/Location/106318430514", "quantity": 7}]}}),
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
        json!({"input": {"reason": "correction", "referenceDocumentUri": "logistics://har-305/move/blocked", "changes": [{"inventoryItemId": "gid://shopify/InventoryItem/53204673823026", "quantity": 1, "from": {"locationId": "gid://shopify/Location/106318430514", "name": "available"}, "to": {"locationId": "gid://shopify/Location/106318463282", "name": "damaged", "ledgerDocumentUri": "ledger://har-305/move/blocked"}}]}}),
    ));
    assert_eq!(
        blocked_move.body["data"]["inventoryMoveQuantities"]["userErrors"],
        json!([{"field": ["input", "changes", "0"], "message": "The quantities can't be moved between different locations."}])
    );
}

#[test]
fn inventory_adjust_quantities_stages_levels_logs_and_reads_back_by_root_field() {
    let mut proxy = snapshot_proxy();

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
            {"inventoryItemId": "gid://shopify/InventoryItem/store-backed", "locationId": "gid://shopify/Location/1", "delta": 5, "changeFromQuantity": 0}
        ]}}),
    ));
    assert_eq!(
        adjust.body["data"]["adjust"]["inventoryAdjustmentGroup"]["changes"][0],
        json!({"name": "available", "delta": 5, "item": {"id": "gid://shopify/InventoryItem/store-backed"}, "location": {"id": "gid://shopify/Location/1", "name": "Source location"}})
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
        json!({"id": "gid://shopify/InventoryItem/store-backed"}),
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
        json!({"id": "gid://shopify/InventoryItem/store-backed"})
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
            {"inventoryItemId": "gid://shopify/InventoryItem/store-backed", "locationId": "gid://shopify/Location/1", "delta": 1, "changeFromQuantity": 5}
        ]}}),
    ));
    assert_eq!(
        invalid_reason.body["data"]["inventoryAdjustQuantities"]["userErrors"][0]["code"],
        json!("INVALID_REASON")
    );

    let log = proxy.get_log_snapshot();
    assert_eq!(
        log["entries"][0]["interpreted"]["operationName"],
        json!("inventoryAdjustQuantities")
    );
    assert_eq!(log["entries"][0]["status"], json!("staged"));
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
            {"inventoryItemId": "gid://shopify/InventoryItem/missing-change", "locationId": "gid://shopify/Location/1", "delta": 1}
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
    assert_eq!(proxy.get_log_snapshot()["entries"], json!([]));
}

#[test]
fn order_create_decrements_inventory_when_inventory_behaviour_is_not_bypass() {
    let mut proxy = snapshot_proxy();

    let seed = proxy.process_request(json_graphql_request(
        r#"
        mutation SeedInventory($input: InventorySetQuantitiesInput!) {
          inventorySetQuantities(input: $input) {
            userErrors { field message }
          }
        }
        "#,
        json!({"input": {"name": "available", "reason": "correction", "referenceDocumentUri": "logistics://inventory/order-create-seed", "ignoreCompareQuantity": true, "quantities": [
            {"inventoryItemId": "gid://shopify/InventoryItem/order-create-decrement", "locationId": "gid://shopify/Location/1", "quantity": 5}
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
                    "variantId": "gid://shopify/ProductVariant/order-create-decrement",
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
        json!({"id": "gid://shopify/InventoryItem/order-create-decrement"}),
    ));
    assert_eq!(
        read.body["data"]["inventoryItem"]["variant"]["inventoryQuantity"],
        json!(3)
    );
    assert_eq!(
        read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"],
        json!([
            {"name": "available", "quantity": 3},
            {"name": "on_hand", "quantity": 3}
        ])
    );
    let log = proxy.get_log_snapshot();
    assert_eq!(
        log["entries"][1]["interpreted"]["operationName"],
        json!("orderCreate")
    );
    assert_eq!(log["entries"][1]["status"], json!("staged"));
    assert_eq!(
        log["entries"][1]["interpreted"]["capability"],
        json!({
            "operationName": "orderCreate",
            "domain": "orders",
            "execution": "stage-locally"
        })
    );
    assert_eq!(
        log["entries"][1]["notes"],
        json!("Locally staged orderCreate in shopify-draft-proxy.")
    );
    assert_eq!(
        log["entries"][1]["stagedResourceIds"],
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
            {"inventoryItemId": "gid://shopify/InventoryItem/order-create-bypass", "locationId": "gid://shopify/Location/1", "quantity": 8}
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
                    "variantId": "gid://shopify/ProductVariant/order-create-bypass",
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
        json!({"id": "gid://shopify/InventoryItem/order-create-bypass"}),
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

#[test]
fn inventory_transfer_lifecycle_stages_and_updates_inventory_levels_from_store() {
    let mut proxy = snapshot_proxy();

    let create_response = proxy.process_request(json_graphql_request(
        include_str!("../../config/parity-requests/products/inventory-transfer-create.graphql"),
        json!({"input": {
            "originLocationId": "gid://shopify/Location/1",
            "destinationLocationId": "gid://shopify/Location/2",
            "lineItems": [{"inventoryItemId": "gid://shopify/InventoryItem/transfer-item", "quantity": 2}]
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

    let inventory_read = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/inventory-transfer-inventory-read-all-levels.graphql"
        ),
        json!({"id": "gid://shopify/InventoryItem/transfer-item"}),
    ));
    assert_eq!(
        inventory_read.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]["quantities"],
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
    let inventory_after_cancel = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/inventory-transfer-inventory-read.graphql"
        ),
        json!({"id": "gid://shopify/InventoryItem/transfer-item"}),
    ));
    assert_eq!(
        inventory_after_cancel.body["data"]["inventoryItem"]["inventoryLevels"]["nodes"][0]
            ["quantities"],
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

    let log = proxy.get_log_snapshot();
    let roots: Vec<Value> = log["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["interpreted"]["operationName"].clone())
        .collect();
    assert_eq!(
        roots,
        vec![
            json!("inventoryTransferCreate"),
            json!("inventoryTransferMarkAsReadyToShip"),
            json!("inventoryTransferCancel")
        ]
    );
}

#[test]
fn selling_plan_downstream_reads_replay_captured_membership_shapes() {
    let lifecycle: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/selling-plan-group-lifecycle.json"
    ))
    .unwrap();
    let relationship: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2026-04/products/product-relationship-roots.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    let lifecycle_variables = json!({
        "productId": "gid://shopify/Product/10171897807154",
        "variantId": "gid://shopify/ProductVariant/51104286736690"
    });
    for (capture_index, label) in [
        (4_usize, "after create"),
        (6, "after product removal"),
        (10, "after variant add"),
    ] {
        let response = proxy.process_request(json_graphql_request(
            include_str!(
                "../../config/parity-requests/products/selling-plan-group-downstream-read.graphql"
            ),
            lifecycle_variables.clone(),
        ));
        assert_eq!(
            response.body["data"], lifecycle["captures"][capture_index]["response"]["data"],
            "selling plan lifecycle downstream read {label} should match capture"
        );
    }

    let relationship_response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/products/product-relationship-selling-plan-membership-read.graphql"
        ),
        relationship["sellingPlanDownstreamRead"]["variables"].clone(),
    ));
    assert_eq!(
        relationship_response.body["data"],
        relationship["sellingPlanDownstreamRead"]["response"]["data"]
    );
}

#[test]
fn combined_listing_product_create_preserves_captured_parent_roles() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../fixtures/conformance/harry-test-heelo.myshopify.com/2025-01/products/combinedListingUpdate-validation.json"
    ))
    .unwrap();
    let mut proxy = snapshot_proxy();

    for operation_key in ["createParentAlready", "createParentEditRemove"] {
        let response = proxy.process_request(json_graphql_request(
            include_str!(
                "../../config/parity-requests/products/combinedListingUpdate-validation-product-create.graphql"
            ),
            fixture["operations"][operation_key]["request"]["variables"].clone(),
        ));
        assert_eq!(
            response.body["data"], fixture["operations"][operation_key]["response"]["data"],
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
        json!({"first": 2}),
    ));
    assert_eq!(
        first_page.body["data"]["mobilePlatformApplications"],
        json!({
            "nodes": [
                {"__typename": "AppleApplication", "id": "gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic", "appId": "com.example.apple.one"},
                {"__typename": "AndroidApplication", "id": "gid://shopify/MobilePlatformApplication/2?shopify-draft-proxy=synthetic", "applicationId": "com.example.android"}
            ],
            "edges": [
                {"cursor": "gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic", "node": {"__typename": "AppleApplication", "id": "gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic", "appId": "com.example.apple.one"}},
                {"cursor": "gid://shopify/MobilePlatformApplication/2?shopify-draft-proxy=synthetic", "node": {"__typename": "AndroidApplication", "id": "gid://shopify/MobilePlatformApplication/2?shopify-draft-proxy=synthetic", "applicationId": "com.example.android"}}
            ],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": "gid://shopify/MobilePlatformApplication/1?shopify-draft-proxy=synthetic",
                "endCursor": "gid://shopify/MobilePlatformApplication/2?shopify-draft-proxy=synthetic"
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
        json!({"first": 2, "after": first_page.body["data"]["mobilePlatformApplications"]["pageInfo"]["endCursor"]}),
    ));
    assert_eq!(
        second_page.body["data"]["mobilePlatformApplications"],
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
    assert_eq!(proxy.get_log_snapshot(), json!({ "entries": [] }));
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
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

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
    assert_eq!(
        proxy.get_log_snapshot()["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
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
          blank: scriptTagCreate(input: { src: "" }) { scriptTag { id src displayScope } userErrors { code field message } }
          tooLong: scriptTagCreate(input: { src: "https://example.test/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa" }) { scriptTag { id src displayScope } userErrors { code field message } }
          invalid: scriptTagCreate(input: { src: "not-a-url" }) { scriptTag { id src displayScope } userErrors { code field message } }
          http: scriptTagCreate(input: { src: "http://example.test/app.js" }) { scriptTag { id src displayScope } userErrors { code field message } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        script_validation.body["data"]["blank"]["userErrors"][0],
        json!({"code": "BLANK", "field": ["input", "src"], "message": "Source can't be blank"})
    );
    assert_eq!(
        script_validation.body["data"]["tooLong"]["userErrors"][0]["code"],
        json!("TOO_LONG")
    );
    assert_eq!(
        script_validation.body["data"]["invalid"]["userErrors"][0]["code"],
        json!("INVALID")
    );
    assert_eq!(
        script_validation.body["data"]["http"]["userErrors"][0]["code"],
        json!("INVALID")
    );

    let create_script = proxy.process_request(json_graphql_request(
        r#"
        mutation ScriptTagUpdateValidationCreate {
          scriptTagCreate(input: { src: "https://cdn.example.test/app.js", displayScope: ALL }) { scriptTag { id src displayScope event cache } userErrors { code field message } }
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
          scriptTagUpdate(id: "gid://shopify/ScriptTag/1?shopify-draft-proxy=synthetic", input: { event: "onstart", cache: true }) { scriptTag { id src displayScope event cache } userErrors { code field message } }
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
fn online_store_storefront_access_token_edges_ported_from_gleam() {
    let mut proxy = snapshot_proxy();

    let first = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreStorefrontAccessTokenLocalRuntimeFirst {
          storefrontAccessTokenCreate(input: { title: "Hydrogen" }) {
            storefrontAccessToken { id title accessToken accessScopes { handle } }
            shop { id }
            userErrors { code field message }
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
            "shop": {"id": "gid://shopify/Shop/92891250994"},
            "userErrors": []
        })
    );

    let mut filtered_request = json_graphql_request(
        r#"
        mutation RustOnlineStoreStorefrontAccessTokenLocalRuntimeFilteredScopes {
          storefrontAccessTokenCreate(input: { title: "Hydrogen filtered" }) {
            storefrontAccessToken { id title accessToken accessScopes { handle } }
            userErrors { code field message }
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
    assert_eq!(
        filtered.body["data"]["storefrontAccessTokenCreate"]["storefrontAccessToken"]
            ["accessScopes"],
        json!([
            {"handle": "unauthenticated_read_customers"},
            {"handle": "unauthenticated_read_product_inventory"}
        ])
    );

    let blank = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreStorefrontAccessTokenLocalRuntimeBlankTitle {
          storefrontAccessTokenCreate(input: { title: "   " }) {
            storefrontAccessToken { id }
            shop { id }
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        blank.body["data"]["storefrontAccessTokenCreate"],
        json!({
            "storefrontAccessToken": null,
            "shop": {"id": "gid://shopify/Shop/92891250994"},
            "userErrors": [{"code": "BLANK", "field": ["input", "title"], "message": "Title can't be blank"}]
        })
    );

    for index in 0..98 {
        let fill = proxy.process_request(json_graphql_request(
            r#"
            mutation RustOnlineStoreStorefrontAccessTokenLocalRuntimeFill($title: String!) {
              storefrontAccessTokenCreate(input: { title: $title }) {
                storefrontAccessToken { id }
                userErrors { code field message }
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
            userErrors { code field message }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        limit.body["data"]["storefrontAccessTokenCreate"],
        json!({
            "storefrontAccessToken": null,
            "userErrors": [{"code": "REACHED_LIMIT", "field": ["input"], "message": "apps.admin.graph_api_errors.storefront_access_token_create.reached_limit"}]
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
fn online_store_pixel_endpoint_edges_ported_from_gleam() {
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

    let server_pixel = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreServerPixelEndpointLocalRuntimeEdges {
          create: serverPixelCreate { serverPixel { id status webhookEndpointAddress } userErrors { __typename code field message } }
          invalidArn: eventBridgeServerPixelUpdate(arn: "not-an-arn") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } }
          blankPubSub: pubSubServerPixelUpdate(pubSubProject: "", pubSubTopic: " ") { serverPixel { id webhookEndpointAddress } userErrors { __typename code field message } }
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
        server_pixel.body["data"]["invalidArn"]["userErrors"][0]["code"],
        json!("INVALID_FIELD_ARGUMENTS")
    );
    assert_eq!(
        server_pixel.body["data"]["blankPubSub"]["userErrors"],
        json!([
            {"__typename": "ServerPixelUserError", "code": "INVALID_FIELD_ARGUMENTS", "field": ["pubSubProject"], "message": "pubSubProject can't be blank"},
            {"__typename": "ServerPixelUserError", "code": "INVALID_FIELD_ARGUMENTS", "field": ["pubSubTopic"], "message": "pubSubTopic can't be blank"}
        ])
    );
    assert_eq!(
        server_pixel.body["data"]["eventBridge"]["serverPixel"]["webhookEndpointAddress"],
        json!("arn:aws:events:us-east-1:123456789012:event-bus/local")
    );
    assert_eq!(
        server_pixel.body["data"]["pubsub"]["serverPixel"]["webhookEndpointAddress"],
        json!("project/topic")
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
fn online_store_theme_lifecycle_tail_helpers_ported_from_gleam() {
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
fn online_store_theme_file_lifecycle_tail_helpers_ported_from_gleam() {
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
          first: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "templates/index.json", body: { type: TEXT, value: "hello" } }]) { upsertedThemeFiles { filename checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } }
          second: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "templates/index.json", body: { type: TEXT, value: "hello world" } }]) { upsertedThemeFiles { filename checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } }
          invalid: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "evil/path.liquid", body: { type: TEXT, value: "ignored" } }]) { upsertedThemeFiles { filename } userErrors { field message code } }
          app: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "assets/app.js", body: { type: TEXT, value: "console.log(1)" } }]) { upsertedThemeFiles { filename } userErrors { field message code } }
          theme: themeFilesUpsert(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ filename: "assets/theme.js", body: { type: TEXT, value: "hello" } }]) { upsertedThemeFiles { filename } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        upserts.body["data"]["first"]["upsertedThemeFiles"][0],
        json!({"filename": "templates/index.json", "checksumMd5": "5d41402abc4b2a76b9719d911017c592", "size": 5, "body": {"content": "hello"}})
    );
    assert_eq!(
        upserts.body["data"]["second"]["upsertedThemeFiles"][0],
        json!({"filename": "templates/index.json", "checksumMd5": "5eb63bbbe01eeed093cb22bb8f5acdc3", "size": 11, "body": {"content": "hello world"}})
    );
    assert_eq!(
        upserts.body["data"]["invalid"],
        json!({"upsertedThemeFiles": [], "userErrors": [{"field": ["files", "0", "filename"], "message": "Filename is invalid", "code": "INVALID"}]})
    );

    let copy_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation RustOnlineStoreThemeFileLocalRuntimeCopyDelete {
          missingCopy: themeFilesCopy(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ srcFilename: "assets/missing.js", dstFilename: "assets/copy.js" }]) { copiedThemeFiles { filename } userErrors { field message code } }
          copy: themeFilesCopy(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ srcFilename: "assets/app.js", dstFilename: "assets/copy.js" }]) { copiedThemeFiles { filename checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } }
          multiCopy: themeFilesCopy(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ srcFilename: "assets/app.js", dstFilename: "assets/app-copy.js" }, { srcFilename: "assets/theme.js", dstFilename: "assets/theme-copy.js" }]) { copiedThemeFiles { filename checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } userErrors { field message code } }
          mixedCopy: themeFilesCopy(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: [{ srcFilename: "assets/missing.js", dstFilename: "assets/missing-copy.js" }, { srcFilename: "assets/theme.js", dstFilename: "assets/theme-copy-2.js" }]) { copiedThemeFiles { filename } userErrors { field message code } }
          requiredDelete: themeFilesDelete(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: ["config/settings_data.json", "config/settings_schema.json"]) { deletedThemeFiles { filename } userErrors { field message code } }
          deleteCopy: themeFilesDelete(themeId: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic", files: ["assets/copy.js"]) { deletedThemeFiles { filename } userErrors { field message code } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        copy_delete.body["data"]["missingCopy"],
        json!({"copiedThemeFiles": [], "userErrors": [{"field": ["files", "0", "srcFilename"], "message": "File not found", "code": "NOT_FOUND"}]})
    );
    assert_eq!(
        copy_delete.body["data"]["copy"]["copiedThemeFiles"][0],
        json!({"filename": "assets/copy.js", "checksumMd5": "6114f5adc373accd7b2051bd87078f62", "size": 14, "body": {"content": "console.log(1)"}})
    );
    assert_eq!(
        copy_delete.body["data"]["multiCopy"],
        json!({"copiedThemeFiles": [
            {"filename": "assets/app-copy.js", "checksumMd5": "6114f5adc373accd7b2051bd87078f62", "size": 14, "body": {"content": "console.log(1)"}},
            {"filename": "assets/theme-copy.js", "checksumMd5": "5d41402abc4b2a76b9719d911017c592", "size": 5, "body": {"content": "hello"}}
        ], "userErrors": []})
    );
    assert_eq!(
        copy_delete.body["data"]["mixedCopy"],
        json!({"copiedThemeFiles": [{"filename": "assets/theme-copy-2.js"}], "userErrors": [{"field": ["files", "0", "srcFilename"], "message": "File not found", "code": "NOT_FOUND"}]})
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
        json!({"deletedThemeFiles": [{"filename": "assets/copy.js"}], "userErrors": []})
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query RustOnlineStoreThemeFileLocalRuntimeRead {
          theme(id: "gid://shopify/OnlineStoreTheme/1?shopify-draft-proxy=synthetic") { files(first: 10) { nodes { filename checksumMd5 size body { ... on OnlineStoreThemeFileBodyText { content } } } } }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        read.body["data"]["theme"]["files"]["nodes"],
        json!([
            {"filename": "templates/index.json", "checksumMd5": "5eb63bbbe01eeed093cb22bb8f5acdc3", "size": 11, "body": {"content": "hello world"}},
            {"filename": "assets/app.js", "checksumMd5": "6114f5adc373accd7b2051bd87078f62", "size": 14, "body": {"content": "console.log(1)"}},
            {"filename": "assets/theme.js", "checksumMd5": "5d41402abc4b2a76b9719d911017c592", "size": 5, "body": {"content": "hello"}},
            {"filename": "assets/app-copy.js", "checksumMd5": "6114f5adc373accd7b2051bd87078f62", "size": 14, "body": {"content": "console.log(1)"}},
            {"filename": "assets/theme-copy.js", "checksumMd5": "5d41402abc4b2a76b9719d911017c592", "size": 5, "body": {"content": "hello"}},
            {"filename": "assets/theme-copy-2.js", "checksumMd5": "5d41402abc4b2a76b9719d911017c592", "size": 5, "body": {"content": "hello"}}
        ])
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
    assert_eq!(created["handle"], json!("normal-operation"));
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
    assert_eq!(
        duplicate.body["data"]["metaobjectCreate"]["metaobject"]["handle"],
        json!("normal-operation-1")
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
            "handle": {"type": "ticket_metaobject_type", "handle": "normal-operation"},
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
            "handle": {"type": "ticket_metaobject_type", "handle": "normal-operation"},
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
    assert_eq!(
        invalid.body["data"]["metaobjectCreate"]["metaobject"],
        Value::Null
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
    let mut proxy = snapshot_proxy();

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
            "files": [{"id": "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic", "alt": "Reference source", "createdAt": "2024-01-01T00:00:01.000Z", "fileStatus": "UPLOADED", "filename": "reference-source.jpg", "image": {"url": "https://cdn.example.com/reference-source.jpg", "width": null, "height": null}}],
            "userErrors": []
        })
    );

    let attach = proxy.process_request(json_graphql_request(
        r#"
        mutation FileReferenceAttach($files: [FileUpdateInput!]!) {
          fileUpdate(files: $files) { files { id alt fileStatus ... on MediaImage { image { url } } } userErrors { field message code } }
        }
        "#,
        json!({"files": [{"id": "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic", "alt": "Attached file media", "originalSource": "https://cdn.example.com/file-reference-ready.jpg", "referencesToAdd": ["gid://shopify/Product/429001"]}]}),
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
        json!({"nodes": [{"id": "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic", "alt": "Reference source", "createdAt": "2024-01-01T00:00:01.000Z", "fileStatus": "UPLOADED", "filename": "reference-source.jpg", "image": {"url": "https://cdn.example.com/reference-source.jpg", "width": null, "height": null}}], "pageInfo": {"hasNextPage": false, "hasPreviousPage": false, "startCursor": "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic", "endCursor": "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic"}})
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation FileDeleteParity($fileIds: [ID!]!) {
          fileDelete(fileIds: $fileIds) { deletedFileIds userErrors { field message code } }
        }
        "#,
        json!({"fileIds": ["gid://shopify/MediaImage/39516006482153"]}),
    ));
    assert_eq!(
        delete.body["data"]["fileDelete"],
        json!({"deletedFileIds": ["gid://shopify/MediaImage/39516006482153"], "userErrors": []})
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
                {"id": "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic", "alt": "First"},
                {"id": "gid://shopify/MediaImage/2?shopify-draft-proxy=synthetic", "alt": "Second"}
            ],
            "edges": [
                {"cursor": "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic", "node": {"id": "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic", "alt": "First"}},
                {"cursor": "gid://shopify/MediaImage/2?shopify-draft-proxy=synthetic", "node": {"id": "gid://shopify/MediaImage/2?shopify-draft-proxy=synthetic", "alt": "Second"}}
            ],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": false,
                "startCursor": "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic",
                "endCursor": "gid://shopify/MediaImage/2?shopify-draft-proxy=synthetic"
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
            "nodes": [{"id": "gid://shopify/MediaImage/3?shopify-draft-proxy=synthetic", "alt": "Third"}],
            "edges": [{"cursor": "gid://shopify/MediaImage/3?shopify-draft-proxy=synthetic", "node": {"id": "gid://shopify/MediaImage/3?shopify-draft-proxy=synthetic", "alt": "Third"}}],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": true,
                "startCursor": "gid://shopify/MediaImage/3?shopify-draft-proxy=synthetic",
                "endCursor": "gid://shopify/MediaImage/3?shopify-draft-proxy=synthetic"
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
        json!({"last": 1, "before": "gid://shopify/MediaImage/3?shopify-draft-proxy=synthetic"}),
    ));
    assert_eq!(
        before_tail.body["data"]["files"],
        json!({
            "nodes": [{"id": "gid://shopify/MediaImage/2?shopify-draft-proxy=synthetic", "alt": "Second"}],
            "edges": [{"cursor": "gid://shopify/MediaImage/2?shopify-draft-proxy=synthetic", "node": {"id": "gid://shopify/MediaImage/2?shopify-draft-proxy=synthetic", "alt": "Second"}}],
            "pageInfo": {
                "hasNextPage": true,
                "hasPreviousPage": true,
                "startCursor": "gid://shopify/MediaImage/2?shopify-draft-proxy=synthetic",
                "endCursor": "gid://shopify/MediaImage/2?shopify-draft-proxy=synthetic"
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
    assert_eq!(
        first_id,
        "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic"
    );
    assert_eq!(
        second_id,
        "gid://shopify/MediaImage/2?shopify-draft-proxy=synthetic"
    );
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
            {"id": first_id, "alt": "First batch", "createdAt": "2024-01-01T00:00:01.000Z", "fileStatus": "UPLOADED", "filename": "first.jpg"},
            {"id": second_id, "alt": "Second batch", "createdAt": "2024-01-01T00:00:01.000Z", "fileStatus": "UPLOADED", "filename": "second.jpg"}
        ])
    );
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
        json!({"files": [{"id": "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic", "fileStatus": "UPLOADED"}], "userErrors": []})
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
fn media_file_update_validates_field_precedence_and_aggregates_missing_ids() {
    let mut proxy = snapshot_proxy();
    let mutation = r#"
        mutation MediaFileUpdateValidation($files: [FileUpdateInput!]!) {
          fileUpdate(files: $files) {
            files { id fileStatus alt }
            userErrors { field message code }
          }
        }
    "#;

    let source_conflict = proxy.process_request(json_graphql_request(
        mutation,
        json!({"files": [{"id": "gid://shopify/MediaImage/404", "originalSource": "https://cdn.example.com/source.png", "previewImageSource": "https://cdn.example.com/preview.png"}]}),
    ));
    assert_eq!(
        source_conflict.body["data"]["fileUpdate"],
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
        json!("gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic")
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
        json!({"fileIds": ["gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic"]}),
    ));
    assert_eq!(
        acknowledge_non_ready.body["data"]["fileAcknowledgeUpdateFailed"],
        json!({"files": null, "userErrors": [{
            "field": ["fileIds"],
            "message": "File with id gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic is not in the READY state.",
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
        json!({"fileIds": ["gid://shopify/MediaImage/999", "gid://shopify/MediaImage/1?shopify-draft-proxy=synthetic"]}),
    ));
    assert_eq!(
        acknowledge_missing.body["data"]["fileAcknowledgeUpdateFailed"],
        json!({"files": null, "userErrors": [{
            "field": ["fileIds"],
            "message": "File id gid://shopify/MediaImage/999 does not exist.",
            "code": "FILE_DOES_NOT_EXIST"
        }]})
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
