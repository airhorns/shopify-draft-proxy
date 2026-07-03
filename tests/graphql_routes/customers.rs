use super::common::*;
use pretty_assertions::assert_eq;

fn create_customer(
    proxy: &mut DraftProxy,
    email: &str,
    first_name: &str,
    last_name: &str,
    tags: Vec<String>,
    note: Option<&str>,
) -> String {
    let mut input = json!({
        "email": email,
        "firstName": first_name,
        "lastName": last_name,
        "tags": tags
    });
    if let Some(note) = note {
        input["note"] = json!(note);
    }
    create_customer_from_input(proxy, input)
}

fn create_customer_from_input(proxy: &mut DraftProxy, input: Value) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerCreateParityPlan($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id
              email
              firstName
              lastName
              displayName
              tags
              note
              state
              defaultEmailAddress { emailAddress marketingState }
              emailMarketingConsent { marketingState }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": input }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

fn assert_merge_survivor(
    proxy: &mut DraftProxy,
    one_id: &str,
    two_id: &str,
    override_fields: Value,
    expected_result_id: &str,
    expected_source_id: &str,
) {
    let merge = proxy.process_request(json_graphql_request(
        r#"
        mutation MergeSelection($one: ID!, $two: ID!, $override: CustomerMergeOverrideFields) {
          customerMerge(customerOneId: $one, customerTwoId: $two, overrideFields: $override) {
            resultingCustomerId
            job { id done }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "one": one_id,
            "two": two_id,
            "override": override_fields,
        }),
    ));
    assert_eq!(merge.status, 200);
    assert_eq!(merge.body["data"]["customerMerge"]["userErrors"], json!([]));
    assert_eq!(
        merge.body["data"]["customerMerge"]["resultingCustomerId"],
        json!(expected_result_id)
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query MergeSelectionReadback($result: ID!, $source: ID!) {
          result: customer(id: $result) { id email state defaultEmailAddress { emailAddress } }
          source: customer(id: $source) { id email state }
        }
        "#,
        json!({
            "result": expected_result_id,
            "source": expected_source_id,
        }),
    ));
    assert_eq!(downstream.status, 200);
    assert_eq!(
        downstream.body["data"]["result"]["id"],
        json!(expected_result_id)
    );
    assert_eq!(downstream.body["data"]["source"], Value::Null);

    let state = state_snapshot(proxy);
    assert_eq!(
        state["stagedState"]["mergedCustomerIds"][expected_source_id],
        json!(expected_result_id)
    );
    assert!(state["stagedState"]["deletedCustomerIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| id.as_str() == Some(expected_source_id)));
}

#[test]
fn customer_input_metafields_round_trip_as_owner_metafields() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerMetafieldsRoundTrip($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "metafield-round-trip@example.test",
                "metafields": [
                    { "namespace": "custom", "key": "tier", "type": "single_line_text_field", "value": "gold" },
                    { "namespace": "profile", "key": "birthday", "type": "date", "value": "1990-01-01" }
                ]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    let customer_id = create.body["data"]["customerCreate"]["customer"]["id"]
        .as_str()
        .expect("customer id")
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CustomerMetafieldsRead($id: ID!) {
          customer(id: $id) {
            id
            tier: metafield(namespace: "custom", key: "tier") { namespace key type value }
            birthday: metafield(namespace: "profile", key: "birthday") { namespace key type value }
            metafields(first: 5) {
              nodes { namespace key type value }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({ "id": customer_id }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["customer"]["tier"],
        json!({
            "namespace": "custom",
            "key": "tier",
            "type": "single_line_text_field",
            "value": "gold"
        })
    );
    assert_eq!(
        read.body["data"]["customer"]["birthday"],
        json!({
            "namespace": "profile",
            "key": "birthday",
            "type": "date",
            "value": "1990-01-01"
        })
    );
    assert_eq!(
        read.body["data"]["customer"]["metafields"]["nodes"],
        json!([
            { "namespace": "custom", "key": "tier", "type": "single_line_text_field", "value": "gold" },
            { "namespace": "profile", "key": "birthday", "type": "date", "value": "1990-01-01" }
        ])
    );
    assert_eq!(
        read.body["data"]["customer"]["metafields"]["pageInfo"]["hasNextPage"],
        json!(false)
    );
}

#[test]
fn customers_count_uses_staged_customers_when_no_baseline_exists() {
    let mut proxy = snapshot_proxy();
    create_customer(
        &mut proxy,
        "count-one@example.test",
        "Count",
        "One",
        Vec::new(),
        None,
    );
    create_customer(
        &mut proxy,
        "count-two@example.test",
        "Count",
        "Two",
        Vec::new(),
        None,
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query StagedCustomersCount {
          customersCount { count precision }
        }
        "#,
        json!({}),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["customersCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
}

#[test]
fn customer_merge_stages_and_downstream_reads_are_operation_name_independent() {
    let mut proxy = snapshot_proxy();
    let source_id = create_customer(
        &mut proxy,
        "merge-source@example.test",
        "Merge",
        "Source",
        vec!["source".to_string()],
        Some("source note"),
    );
    let result_id = create_customer(
        &mut proxy,
        "merge-result@example.test",
        "Merge",
        "Result",
        vec!["result".to_string()],
        None,
    );

    let merge = proxy.process_request(json_graphql_request(
        r#"
        mutation TotallyArbitraryMergeName($one: ID!, $two: ID!, $override: CustomerMergeOverrideFields) {
          customerMerge(customerOneId: $one, customerTwoId: $two, overrideFields: $override) {
            resultingCustomerId
            job { id done }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "one": source_id,
            "two": result_id,
            "override": {
                "customerIdOfEmailToKeep": result_id,
                "customerIdOfFirstNameToKeep": source_id,
                "customerIdOfLastNameToKeep": result_id,
                "note": "merged note",
                "tags": ["merged", "source", "result"]
            }
        }),
    ));
    assert_eq!(merge.status, 200);
    assert_eq!(
        merge.body["data"]["customerMerge"]["resultingCustomerId"],
        json!(result_id)
    );
    assert_eq!(merge.body["data"]["customerMerge"]["userErrors"], json!([]));
    assert_eq!(
        merge.body["data"]["customerMerge"]["job"]["done"],
        json!(false)
    );
    let job_id = merge.body["data"]["customerMerge"]["job"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query MergeReadAfterWrite($source: ID!, $result: ID!, $sourceEmail: String!, $resultEmail: String!, $job: ID!) {
          source: customer(id: $source) { id email }
          result: customer(id: $result) { id email firstName lastName displayName note tags defaultEmailAddress { emailAddress } }
          bySourceEmail: customerByIdentifier(identifier: { emailAddress: $sourceEmail }) { id email }
          byResultEmail: customerByIdentifier(identifier: { emailAddress: $resultEmail }) { id email defaultEmailAddress { emailAddress } }
          customers(first: 5) { nodes { id email } pageInfo { hasNextPage hasPreviousPage } }
          customersCount { count precision }
          mergeStatus: customerMergeJobStatus(jobId: $job) {
            jobId
            resultingCustomerId
            status
            customerMergeErrors { errorFields message }
          }
          job(id: $job) { id done }
          node(id: $job) { ... on Job { id done } }
        }
        "#,
        json!({
            "source": source_id,
            "result": result_id,
            "sourceEmail": "merge-source@example.test",
            "resultEmail": "merge-result@example.test",
            "job": job_id
        }),
    ));
    assert_eq!(downstream.status, 200);
    assert_eq!(downstream.body["data"]["source"], Value::Null);
    assert_eq!(
        downstream.body["data"]["result"],
        json!({
            "id": result_id,
            "email": "merge-result@example.test",
            "firstName": "Merge",
            "lastName": "Result",
            "displayName": "Merge Result",
            "note": "merged note",
            "tags": ["merged", "result", "source"],
            "defaultEmailAddress": { "emailAddress": "merge-result@example.test" }
        })
    );
    assert_eq!(downstream.body["data"]["bySourceEmail"], Value::Null);
    assert_eq!(
        downstream.body["data"]["byResultEmail"]["id"],
        json!(result_id)
    );
    assert_eq!(
        downstream.body["data"]["customers"]["nodes"],
        json!([{ "id": result_id, "email": "merge-result@example.test" }])
    );
    assert_eq!(
        downstream.body["data"]["customersCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        downstream.body["data"]["mergeStatus"],
        json!({
            "jobId": job_id,
            "resultingCustomerId": result_id,
            "status": "COMPLETED",
            "customerMergeErrors": []
        })
    );
    assert_eq!(downstream.body["data"]["job"]["id"], json!(job_id));
    assert_eq!(downstream.body["data"]["job"]["done"], json!(true));
    assert_eq!(downstream.body["data"]["node"]["id"], json!(job_id));
    assert_eq!(downstream.body["data"]["node"]["done"], json!(true));

    let state = state_snapshot(&proxy);
    assert_eq!(
        state["stagedState"]["mergedCustomerIds"][source_id.as_str()],
        json!(result_id)
    );
    assert_eq!(
        state["stagedState"]["customerMergeRequests"][job_id.as_str()]["resultingCustomerId"],
        json!(result_id)
    );
    assert_eq!(
        state["stagedState"]["deletedCustomerIds"],
        json!([source_id])
    );
    let log = log_snapshot(&proxy);
    assert_eq!(
        log["entries"][2]["interpreted"]["primaryRootField"],
        json!("customerMerge")
    );
    assert!(log["entries"][2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("TotallyArbitraryMergeName"));
}

#[test]
fn customer_merge_selects_survivor_from_email_and_state_rules() {
    let mut proxy = snapshot_proxy();
    let one_id = create_customer(
        &mut proxy,
        "merge-override-one@example.test",
        "Override",
        "One",
        Vec::new(),
        None,
    );
    let two_id = create_customer(
        &mut proxy,
        "merge-override-two@example.test",
        "Override",
        "Two",
        Vec::new(),
        None,
    );
    assert_merge_survivor(
        &mut proxy,
        &one_id,
        &two_id,
        json!({ "customerIdOfEmailToKeep": one_id.clone() }),
        &one_id,
        &two_id,
    );

    let mut proxy = snapshot_proxy();
    let one_id = create_customer(
        &mut proxy,
        "merge-single-email-one@example.test",
        "SingleEmail",
        "One",
        Vec::new(),
        None,
    );
    let two_id = create_customer_from_input(
        &mut proxy,
        json!({
            "firstName": "SingleEmail",
            "lastName": "Two"
        }),
    );
    assert_merge_survivor(&mut proxy, &one_id, &two_id, Value::Null, &one_id, &two_id);

    let mut proxy = snapshot_proxy();
    let one_id = create_customer_from_input(
        &mut proxy,
        json!({
            "email": "merge-subscribed-one@example.test",
            "firstName": "Subscribed",
            "lastName": "One",
            "emailMarketingConsent": {
                "marketingState": "SUBSCRIBED",
                "marketingOptInLevel": "SINGLE_OPT_IN",
                "consentUpdatedAt": "2026-04-25T02:10:00Z"
            }
        }),
    );
    let two_id = create_customer(
        &mut proxy,
        "merge-subscribed-two@example.test",
        "Subscribed",
        "Two",
        Vec::new(),
        None,
    );
    assert_merge_survivor(&mut proxy, &one_id, &two_id, Value::Null, &one_id, &two_id);

    let mut proxy = snapshot_proxy();
    let one_id = create_customer_from_input(
        &mut proxy,
        json!({
            "firstName": "NoEmail",
            "lastName": "One"
        }),
    );
    let two_id = create_customer_from_input(
        &mut proxy,
        json!({
            "firstName": "NoEmail",
            "lastName": "Two"
        }),
    );
    assert_merge_survivor(&mut proxy, &one_id, &two_id, Value::Null, &two_id, &one_id);
}

#[test]
fn customer_merge_validations_and_blockers_return_shopify_shaped_errors() {
    let mut proxy = snapshot_proxy();
    let first_id = create_customer(
        &mut proxy,
        "merge-validation-one@example.test",
        "Validation",
        "One",
        vec!["one".to_string()],
        None,
    );
    let second_id = create_customer(
        &mut proxy,
        "merge-validation-two@example.test",
        "Validation",
        "Two",
        vec!["two".to_string()],
        None,
    );

    let self_merge = proxy.process_request(json_graphql_request(
        r#"
        mutation ArbitrarySelfMerge($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id done }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "one": first_id, "two": first_id }),
    ));
    assert_eq!(
        self_merge.body["data"]["customerMerge"],
        json!({
            "resultingCustomerId": null,
            "job": null,
            "userErrors": [{
                "field": null,
                "message": "Customers IDs should not match",
                "code": "INVALID_CUSTOMER_ID"
            }]
        })
    );

    let unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation ArbitraryUnknownMerge($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id done }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "one": first_id,
            "two": "gid://shopify/Customer/999999999999999"
        }),
    ));
    assert_eq!(
        unknown.body["data"]["customerMerge"]["userErrors"],
        json!([{
            "field": ["customerTwoId"],
            "message": "Customer does not exist with ID 999999999999999",
            "code": "INVALID_CUSTOMER_ID"
        }])
    );

    let duplicated_unknown = proxy.process_request(json_graphql_request(
        r#"
        mutation ArbitraryDuplicatedUnknownMerge($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id done }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "one": "gid://shopify/Customer/999999999999999",
            "two": "gid://shopify/Customer/999999999999999"
        }),
    ));
    assert_eq!(
        duplicated_unknown.body["data"]["customerMerge"]["userErrors"],
        json!([{
            "field": ["customerOneId"],
            "message": "Customer does not exist with ID 999999999999999",
            "code": "INVALID_CUSTOMER_ID"
        }])
    );

    let missing = proxy.process_request(json_graphql_request(
        r#"
        mutation MissingArgumentNameDoesNotMatter($one: ID!) {
          customerMerge(customerOneId: $one) {
            resultingCustomerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "one": first_id }),
    ));
    assert_eq!(
        missing.body["errors"][0]["extensions"]["code"],
        json!("missingRequiredArguments")
    );
    assert_eq!(
        missing.body["errors"][0]["extensions"]["arguments"],
        json!("customerTwoId")
    );

    let blank = proxy.process_request(json_graphql_request(
        r#"
        mutation BlankLiteralNameDoesNotMatter {
          customerMerge(customerOneId: "", customerTwoId: "") {
            resultingCustomerId
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(blank.body["errors"].as_array().unwrap().len(), 2);
    assert_eq!(
        blank.body["errors"][0]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );
    assert_eq!(
        blank.body["errors"][1]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );

    let tag_one = create_customer(
        &mut proxy,
        "merge-tags-one@example.test",
        "Tags",
        "One",
        (0..126).map(|index| format!("tag-a-{index}")).collect(),
        None,
    );
    let tag_two = create_customer(
        &mut proxy,
        "merge-tags-two@example.test",
        "Tags",
        "Two",
        (0..126).map(|index| format!("tag-b-{index}")).collect(),
        None,
    );
    let tags_overflow = proxy.process_request(json_graphql_request(
        r#"
        mutation TagsBlockerNameDoesNotMatter($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "one": tag_one, "two": tag_two }),
    ));
    assert_eq!(
        tags_overflow.body["data"]["customerMerge"],
        json!({
            "resultingCustomerId": null,
            "job": null,
            "userErrors": [
                {
                    "field": ["customerOneId"],
                    "message": "Customers must have 250 tags or less.",
                    "code": "INVALID_CUSTOMER"
                },
                {
                    "field": ["customerTwoId"],
                    "message": "Customers must have 250 tags or less.",
                    "code": "INVALID_CUSTOMER"
                }
            ]
        })
    );

    let note_one = create_customer(
        &mut proxy,
        "merge-note-one@example.test",
        "Note",
        "One",
        Vec::new(),
        Some(&"a".repeat(2501)),
    );
    let note_two = create_customer(
        &mut proxy,
        "merge-note-two@example.test",
        "Note",
        "Two",
        Vec::new(),
        Some(&"b".repeat(2500)),
    );
    let note_overflow = proxy.process_request(json_graphql_request(
        r#"
        mutation NotesBlockerNameDoesNotMatter($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "one": note_one, "two": note_two }),
    ));
    assert_eq!(
        note_overflow.body["data"]["customerMerge"]["userErrors"],
        json!([
            {
                "field": ["customerOneId"],
                "message": "Customer notes must be 5,000 characters or less.",
                "code": "INVALID_CUSTOMER"
            },
            {
                "field": ["customerTwoId"],
                "message": "Customer notes must be 5,000 characters or less.",
                "code": "INVALID_CUSTOMER"
            }
        ])
    );

    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["mergedCustomerIds"],
        json!({})
    );
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["customers"][second_id.as_str()]["email"],
        json!("merge-validation-two@example.test")
    );
}

#[test]
fn customer_data_erasure_request_and_cancel_stage_sensitive_side_effects() {
    let mut proxy = snapshot_proxy();
    let customer_id = create_customer(
        &mut proxy,
        "data-erasure@example.test",
        "Data",
        "Erasure",
        vec!["erasure".to_string()],
        None,
    );

    let request = proxy.process_request(json_graphql_request(
        r#"
        mutation NotTheCapturedRequestName($customerId: ID!) {
          customerRequestDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": customer_id }),
    ));
    assert_eq!(
        request.body["data"]["customerRequestDataErasure"],
        json!({ "customerId": customer_id, "userErrors": [] })
    );
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["customerDataErasureRequests"][customer_id.as_str()]
            ["status"],
        json!("REQUESTED")
    );

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query DataErasureLeavesCustomerReadable($id: ID!) {
          customer(id: $id) { id email tags defaultEmailAddress { emailAddress } }
        }
        "#,
        json!({ "id": customer_id }),
    ));
    assert_eq!(
        downstream.body["data"]["customer"]["email"],
        json!("data-erasure@example.test")
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation AlsoNotTheCapturedCancelName($customerId: ID!) {
          customerCancelDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": customer_id }),
    ));
    assert_eq!(
        cancel.body["data"]["customerCancelDataErasure"],
        json!({ "customerId": customer_id, "userErrors": [] })
    );
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["customerDataErasureRequests"][customer_id.as_str()]
            ["status"],
        json!("CANCELED")
    );

    let repeat_cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation RepeatCancel($customerId: ID!) {
          customerCancelDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": customer_id }),
    ));
    assert_eq!(
        repeat_cancel.body["data"]["customerCancelDataErasure"],
        json!({
            "customerId": null,
            "userErrors": [{
                "field": ["customerId"],
                "message": "Customer's data is not scheduled for erasure",
                "code": "NOT_BEING_ERASED"
            }]
        })
    );

    for root in [
        "customerRequestDataErasure",
        "customerCancelDataErasure",
        "customerCancelDataErasure",
    ] {
        assert!(log_snapshot(&proxy)["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["interpreted"]["primaryRootField"] == json!(root)));
    }
    let log = log_snapshot(&proxy);
    assert!(log["entries"][1]["rawBody"]
        .as_str()
        .unwrap()
        .contains("NotTheCapturedRequestName"));
    assert!(log["entries"][2]["rawBody"]
        .as_str()
        .unwrap()
        .contains("AlsoNotTheCapturedCancelName"));

    let unknown_request = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownRequest($customerId: ID!) {
          customerRequestDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": "gid://shopify/Customer/999999999999999" }),
    ));
    assert_eq!(
        unknown_request.body["data"]["customerRequestDataErasure"],
        json!({
            "customerId": null,
            "userErrors": [{
                "field": ["customerId"],
                "message": "Customer does not exist",
                "code": "DOES_NOT_EXIST"
            }]
        })
    );

    let unknown_cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation UnknownCancel($customerId: ID!) {
          customerCancelDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": "gid://shopify/Customer/999999999999999" }),
    ));
    assert_eq!(
        unknown_cancel.body["data"]["customerCancelDataErasure"],
        json!({
            "customerId": null,
            "userErrors": [{
                "field": ["customerId"],
                "message": "Customer does not exist",
                "code": "DOES_NOT_EXIST"
            }]
        })
    );
}

#[test]
fn customer_data_erasure_hydrates_real_customer_before_does_not_exist() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured = Arc::clone(&upstream_calls);
    let customer_id = "gid://shopify/Customer/6543210987";
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream JSON body");
            captured.lock().unwrap().push(body.clone());
            assert_eq!(body["operationName"], json!("CustomerHydrate"));
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "customer": {
                            "id": customer_id,
                            "firstName": "Hydrated",
                            "lastName": "Erasure",
                            "displayName": "Hydrated Erasure",
                            "email": "hydrated-erasure@example.com",
                            "phone": null,
                            "locale": "en",
                            "note": null,
                            "canDelete": true,
                            "verifiedEmail": true,
                            "dataSaleOptOut": false,
                            "taxExempt": false,
                            "taxExemptions": [],
                            "state": "DISABLED",
                            "tags": [],
                            "createdAt": "2026-06-01T00:00:00Z",
                            "updatedAt": "2026-06-01T00:00:00Z",
                            "defaultEmailAddress": { "emailAddress": "hydrated-erasure@example.com" },
                            "defaultPhoneNumber": null,
                            "defaultAddress": null,
                            "addressesV2": { "nodes": [] }
                        }
                    }
                }),
            }
        });

    let request = proxy.process_request(json_graphql_request(
        r#"
        mutation HydratedCustomerDataErasure($customerId: ID!) {
          customerRequestDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": customer_id }),
    ));
    assert_eq!(
        request.body["data"]["customerRequestDataErasure"],
        json!({ "customerId": customer_id, "userErrors": [] })
    );
    assert_eq!(upstream_calls.lock().unwrap().len(), 1);
    assert_eq!(
        state_snapshot(&proxy)["stagedState"]["customerDataErasureRequests"][customer_id]["status"],
        json!("REQUESTED")
    );
}

#[test]
fn customer_address_accepts_supported_country_outside_original_subset() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerAddressDenmark($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              defaultAddress { city country countryCodeV2 province provinceCode formattedArea }
              addressesV2(first: 3) {
                nodes { city country countryCodeV2 province provinceCode formattedArea }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "denmark-address@example.test",
                "addresses": [{
                    "address1": "Radhuspladsen 1",
                    "city": "Copenhagen",
                    "countryCode": "DK",
                    "zip": "1550"
                }]
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["customerCreate"]["customer"]["defaultAddress"],
        json!({
            "city": "Copenhagen",
            "country": "Denmark",
            "countryCodeV2": "DK",
            "province": null,
            "provinceCode": null,
            "formattedArea": "Copenhagen, Denmark"
        })
    );
}

#[test]
fn customer_order_create_allocates_unique_ids_for_example_test_emails() {
    let mut proxy = snapshot_proxy();
    let create_order = |proxy: &mut DraftProxy, email: &str| {
        let response = proxy.process_request(json_graphql_request(
            r#"
            mutation CustomerOrderCreateId($order: OrderCreateOrderInput!) {
              orderCreate(order: $order) {
                order { id }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "order": {
                    "email": email,
                    "currency": "USD",
                    "lineItems": [{ "title": "Synthetic ID line", "quantity": 1 }]
                }
            }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["orderCreate"]["userErrors"],
            json!([])
        );
        response.body["data"]["orderCreate"]["order"]["id"]
            .as_str()
            .expect("order id")
            .to_string()
    };

    let first_id = create_order(&mut proxy, "first-order@example.test");
    let second_id = create_order(&mut proxy, "second-order@example.test");
    assert_ne!(first_id, second_id);
    assert!(first_id.starts_with("gid://shopify/Order/"));
    assert!(second_id.starts_with("gid://shopify/Order/"));
}

#[test]
fn customer_merge_and_erasure_roots_do_not_write_upstream_in_live_hybrid() {
    let forwarded = Arc::new(Mutex::new(Vec::<Request>::new()));
    let captured = Arc::clone(&forwarded);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            captured.lock().unwrap().push(request);
            Response {
                status: 500,
                headers: Default::default(),
                body: json!({ "errors": [{ "message": "unexpected upstream call" }] }),
            }
        });
    let first_id = create_customer(
        &mut proxy,
        "local-only-one@example.test",
        "Local",
        "One",
        Vec::new(),
        None,
    );
    let second_id = create_customer(
        &mut proxy,
        "local-only-two@example.test",
        "Local",
        "Two",
        Vec::new(),
        None,
    );
    // `create_customer` issues a `CustomerDuplicateHydrate` upstream lookup per
    // create in LiveHybrid mode (the duplicate-contact detection path); those
    // are legitimate read-throughs and are parity-recorded. Capture the setup
    // baseline so the assertion isolates the merge/erasure roots, which must
    // never forward upstream.
    let setup_forwards = forwarded.lock().unwrap().len();

    let merge = proxy.process_request(json_graphql_request(
        r#"
        mutation LocalOnlyMerge($one: ID!, $two: ID!) {
          customerMerge(customerOneId: $one, customerTwoId: $two) {
            resultingCustomerId
            job { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "one": first_id, "two": second_id }),
    ));
    assert_eq!(merge.status, 200);
    assert_eq!(merge.body["data"]["customerMerge"]["userErrors"], json!([]));

    let request = proxy.process_request(json_graphql_request(
        r#"
        mutation LocalOnlyErase($customerId: ID!) {
          customerRequestDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": second_id }),
    ));
    assert_eq!(
        request.body["data"]["customerRequestDataErasure"]["userErrors"],
        json!([])
    );

    let cancel = proxy.process_request(json_graphql_request(
        r#"
        mutation LocalOnlyCancel($customerId: ID!) {
          customerCancelDataErasure(customerId: $customerId) {
            customerId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "customerId": second_id }),
    ));
    assert_eq!(
        cancel.body["data"]["customerCancelDataErasure"]["userErrors"],
        json!([])
    );
    assert_eq!(forwarded.lock().unwrap().len(), setup_forwards);
}
