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

fn create_customer_address(proxy: &mut DraftProxy, customer_id: &str, address1: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateCustomerAddress($customerId: ID!, $address: MailingAddressInput!) {
          customerAddressCreate(customerId: $customerId, address: $address, setAsDefault: true) {
            address { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "customerId": customer_id,
            "address": {
                "address1": address1,
                "city": "Ottawa",
                "countryCode": "CA",
                "provinceCode": "ON",
                "zip": "K1A 0B1"
            }
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["customerAddressCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["customerAddressCreate"]["address"]["id"]
        .as_str()
        .expect("address id")
        .to_string()
}

#[test]
fn customer_update_and_set_preserve_hydrated_fields_when_input_omits_them() {
    for root in ["customerUpdate", "customerSet"] {
        let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
        let captured = Arc::clone(&upstream_calls);
        let customer_id = format!("gid://shopify/Customer/{root}");
        let hydrated_customer_id = customer_id.clone();
        let mut proxy =
            configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
                let body: Value = serde_json::from_str(&request.body).expect("upstream JSON body");
                captured.lock().unwrap().push(body.clone());
                assert_eq!(body["operationName"], json!("CustomerHydrate"));
                assert_eq!(body["variables"]["id"], json!(hydrated_customer_id));
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "customer": {
                                "id": hydrated_customer_id,
                                "firstName": "Hydrated",
                                "lastName": "Customer",
                                "displayName": "Hydrated Customer",
                                "email": "hydrated-customer@example.com",
                                "phone": null,
                                "locale": "fr",
                                "note": "kept from hydrate",
                                "canDelete": true,
                                "verifiedEmail": true,
                                "dataSaleOptOut": true,
                                "taxExempt": false,
                                "taxExemptions": [],
                                "state": "ENABLED",
                                "tags": ["existing"],
                                "createdAt": "2026-06-01T00:00:00Z",
                                "updatedAt": "2026-06-01T00:00:00Z",
                                "defaultEmailAddress": { "emailAddress": "hydrated-customer@example.com" },
                                "defaultPhoneNumber": null,
                                "defaultAddress": null,
                                "addressesV2": { "nodes": [] }
                            }
                        }
                    }),
                }
            });

        let response = if root == "customerUpdate" {
            proxy.process_request(json_graphql_request(
                r#"
                mutation PreserveHydratedCustomerUpdate($input: CustomerInput!) {
                  customerUpdate(input: $input) {
                    customer { id tags state dataSaleOptOut locale note }
                    userErrors { field message }
                  }
                }
                "#,
                json!({ "input": { "id": customer_id, "tags": ["vip"] } }),
            ))
        } else {
            proxy.process_request(json_graphql_request(
                r#"
                mutation PreserveHydratedCustomerSet($input: CustomerSetInput!, $identifier: CustomerSetIdentifiers) {
                  customerSet(input: $input, identifier: $identifier) {
                    customer { id tags state dataSaleOptOut locale note }
                    userErrors { field message }
                  }
                }
                "#,
                json!({
                    "identifier": { "id": customer_id },
                    "input": { "tags": ["vip"] }
                }),
            ))
        };

        assert_eq!(response.status, 200);
        let payload = &response.body["data"][root];
        assert_eq!(payload["userErrors"], json!([]));
        assert_eq!(payload["customer"]["tags"], json!(["vip"]));
        assert_eq!(payload["customer"]["state"], json!("ENABLED"));
        assert_eq!(payload["customer"]["dataSaleOptOut"], json!(true));
        assert_eq!(payload["customer"]["locale"], json!("fr"));
        assert_eq!(payload["customer"]["note"], json!("kept from hydrate"));

        let readback = proxy.process_request(json_graphql_request(
            r#"
            query ReadHydratedCustomerAfterUpdate($id: ID!) {
              customer(id: $id) { id tags state dataSaleOptOut locale note }
            }
            "#,
            json!({ "id": customer_id }),
        ));
        assert_eq!(readback.body["data"]["customer"]["state"], json!("ENABLED"));
        assert_eq!(
            readback.body["data"]["customer"]["dataSaleOptOut"],
            json!(true)
        );
        assert_eq!(upstream_calls.lock().unwrap().len(), 1);
    }
}

#[test]
fn customer_create_uses_shop_locale_and_zero_money_order_summary() {
    let mut proxy = snapshot_proxy();
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"]["currencyCode"] = json!("CAD");
        state["baseState"]["shopLocales"] = json!({
            "en": {
                "locale": "en",
                "name": "English",
                "primary": false,
                "published": true,
                "marketWebPresences": []
            },
            "fr": {
                "locale": "fr",
                "name": "French",
                "primary": true,
                "published": true,
                "marketWebPresences": []
            }
        });
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerCreateShopDefaults($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id
              email
              locale
              numberOfOrders
              amountSpent { amount currencyCode }
              lastOrder { id }
              orders(first: 1) {
                nodes { id }
                pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "input": { "email": "shop-defaults@example.test" } }),
    ));

    assert_eq!(response.status, 200);
    let customer = &response.body["data"]["customerCreate"]["customer"];
    assert_eq!(
        response.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(customer["locale"], json!("fr"));
    assert_eq!(customer["numberOfOrders"], json!("0"));
    assert_eq!(
        customer["amountSpent"],
        json!({ "amount": "0.0", "currencyCode": "CAD" })
    );
    assert_eq!(customer["lastOrder"], Value::Null);
    assert_eq!(customer["orders"]["nodes"], json!([]));
    assert_eq!(
        customer["orders"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": null,
            "endCursor": null
        })
    );
}

#[test]
fn customer_create_amount_spent_hydrates_shop_currency_in_live_hybrid() {
    let upstream_calls = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured = Arc::clone(&upstream_calls);
    let mut proxy =
        configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(move |request| {
            let body: Value = serde_json::from_str(&request.body).expect("upstream JSON body");
            captured.lock().unwrap().push(body.clone());
            let query = body["query"].as_str().expect("upstream query");
            if query.contains("CustomerDuplicateHydrate") {
                return Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({ "data": { "customers": { "nodes": [] } } }),
                };
            }
            assert_eq!(
                query,
                "query DraftProxyShopPricingHydrate { shop { currencyCode taxesIncluded taxShipping } }"
            );
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "shop": {
                            "currencyCode": "CAD",
                            "taxesIncluded": false,
                            "taxShipping": false
                        }
                    }
                }),
            }
        });

    let response = proxy.process_request(json_graphql_request(
        include_str!(
            "../../config/parity-requests/customers/customer-order-summary-create-customer.graphql"
        ),
        json!({
            "input": {
                "email": "amount-spent-currency@example.test",
                "firstName": "HAR-288",
                "lastName": "Order Summary"
            }
        }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        response.body["data"]["customerCreate"]["customer"]["amountSpent"],
        json!({ "amount": "0.0", "currencyCode": "CAD" })
    );
    let calls = upstream_calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(calls.iter().any(|body| {
        body["query"].as_str()
            == Some(
                "query DraftProxyShopPricingHydrate { shop { currencyCode taxesIncluded taxShipping } }",
            )
    }));
}

#[test]
fn customer_update_and_set_preserve_created_customer_order_summary_defaults() {
    for root in ["customerUpdate", "customerSet"] {
        let mut proxy = snapshot_proxy();
        restore_shop_currency(&mut proxy, "CAD");
        let create = proxy.process_request(json_graphql_request(
            r#"
            mutation CustomerCreateForOrderSummaryUpdate($input: CustomerInput!) {
              customerCreate(input: $input) {
                customer { id }
                userErrors { field message }
              }
            }
            "#,
            json!({ "input": { "email": format!("order-summary-{root}@example.test") } }),
        ));
        assert_eq!(
            create.body["data"]["customerCreate"]["userErrors"],
            json!([])
        );
        let customer_id = create.body["data"]["customerCreate"]["customer"]["id"]
            .as_str()
            .expect("customer id")
            .to_string();

        let response = if root == "customerUpdate" {
            proxy.process_request(json_graphql_request(
                r#"
                mutation CustomerUpdatePreservesOrderSummary($input: CustomerInput!) {
                  customerUpdate(input: $input) {
                    customer {
                      id
                      tags
                      numberOfOrders
                      amountSpent { amount currencyCode }
                      lastOrder { id }
                      orders(first: 1) {
                        nodes { id }
                        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                      }
                    }
                    userErrors { field message }
                  }
                }
                "#,
                json!({ "input": { "id": customer_id, "tags": ["vip"] } }),
            ))
        } else {
            proxy.process_request(json_graphql_request(
                r#"
                mutation CustomerSetPreservesOrderSummary($input: CustomerSetInput!, $identifier: CustomerSetIdentifiers) {
                  customerSet(input: $input, identifier: $identifier) {
                    customer {
                      id
                      tags
                      numberOfOrders
                      amountSpent { amount currencyCode }
                      lastOrder { id }
                      orders(first: 1) {
                        nodes { id }
                        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
                      }
                    }
                    userErrors { field message }
                  }
                }
                "#,
                json!({
                    "identifier": { "id": customer_id },
                    "input": { "tags": ["vip"] }
                }),
            ))
        };

        assert_eq!(response.status, 200);
        let customer = &response.body["data"][root]["customer"];
        assert_eq!(response.body["data"][root]["userErrors"], json!([]));
        assert_eq!(customer["tags"], json!(["vip"]));
        assert_eq!(customer["numberOfOrders"], json!("0"));
        assert_eq!(
            customer["amountSpent"],
            json!({ "amount": "0.0", "currencyCode": "CAD" })
        );
        assert_eq!(customer["lastOrder"], Value::Null);
        assert_eq!(customer["orders"]["nodes"], json!([]));
        assert_eq!(
            customer["orders"]["pageInfo"],
            json!({
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            })
        );
    }
}

#[test]
fn customer_update_inline_addresses_are_id_aware_and_replace_existing_addresses() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateCustomerWithInlineAddresses($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer {
              id
              defaultAddress { id address1 city }
              addressesV2(first: 5) {
                nodes { id address1 city }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "inline-address-update@example.test",
                "firstName": "Inline",
                "lastName": "Customer",
                "addresses": [
                    {
                        "address1": "100 First St",
                        "city": "San Francisco",
                        "countryCode": "US",
                        "provinceCode": "CA",
                        "zip": "94103"
                    },
                    {
                        "address1": "200 Second St",
                        "city": "Oakland",
                        "countryCode": "US",
                        "provinceCode": "CA",
                        "zip": "94607"
                    }
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
    let initial_nodes = create.body["data"]["customerCreate"]["customer"]["addressesV2"]["nodes"]
        .as_array()
        .expect("address nodes");
    assert_eq!(initial_nodes.len(), 2);
    let address_one_id = initial_nodes[0]["id"]
        .as_str()
        .expect("first address id")
        .to_string();
    let address_two_id = initial_nodes[1]["id"]
        .as_str()
        .expect("second address id")
        .to_string();

    let update_second_only = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateSecondInlineAddress($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer {
              id
              defaultAddress { id address1 city }
              addressesV2(first: 5) {
                nodes { id address1 city }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": customer_id,
                "addresses": [{
                    "id": address_two_id,
                    "address1": "999 Bryant St",
                    "city": "San Francisco",
                    "countryCode": "US",
                    "provinceCode": "CA",
                    "zip": "94103"
                }]
            }
        }),
    ));
    assert_eq!(update_second_only.status, 200);
    assert_eq!(
        update_second_only.body["data"]["customerUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update_second_only.body["data"]["customerUpdate"]["customer"]["addressesV2"]["nodes"],
        json!([{
            "id": address_two_id.clone(),
            "address1": "999 Bryant St",
            "city": "San Francisco"
        }])
    );
    assert_ne!(
        update_second_only.body["data"]["customerUpdate"]["customer"]["addressesV2"]["nodes"][0]
            ["id"],
        json!(address_one_id)
    );
    assert_eq!(
        update_second_only.body["data"]["customerUpdate"]["customer"]["defaultAddress"]["id"],
        json!(address_two_id.clone())
    );

    let omitted_addresses = proxy.process_request(json_graphql_request(
        r#"
        mutation RenameCustomerWithoutAddresses($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer {
              firstName
              addressesV2(first: 5) { nodes { id address1 city } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": customer_id,
                "firstName": "Renamed"
            }
        }),
    ));
    assert_eq!(
        omitted_addresses.body["data"]["customerUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        omitted_addresses.body["data"]["customerUpdate"]["customer"]["addressesV2"]["nodes"],
        json!([{
            "id": address_two_id.clone(),
            "address1": "999 Bryant St",
            "city": "San Francisco"
        }])
    );

    let unknown_id = "gid://shopify/MailingAddress/999999999999";
    let unknown_address = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateUnknownInlineAddress($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id addressesV2(first: 5) { nodes { id address1 city } } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": customer_id,
                "addresses": [{
                    "id": unknown_id,
                    "address1": "Should Not Stage"
                }]
            }
        }),
    ));
    assert_eq!(
        unknown_address.body["data"]["customerUpdate"]["customer"],
        Value::Null
    );
    assert_eq!(
        unknown_address.body["data"]["customerUpdate"]["userErrors"],
        json!([{
            "field": ["addresses", "0", "id"],
            "message": "Customer address does not exist"
        }])
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query ReadCustomerAfterUnknownInlineAddress($id: ID!) {
          customer(id: $id) {
            firstName
            addressesV2(first: 5) { nodes { id address1 city } }
          }
        }
        "#,
        json!({ "id": customer_id }),
    ));
    assert_eq!(
        readback.body["data"]["customer"]["addressesV2"]["nodes"],
        json!([{
            "id": address_two_id,
            "address1": "999 Bryant St",
            "city": "San Francisco"
        }])
    );
    let log = log_snapshot(&proxy);
    assert_eq!(log["entries"].as_array().expect("log entries").len(), 3);
    assert_eq!(
        log["entries"][1]["interpreted"]["primaryRootField"],
        json!("customerUpdate")
    );
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
fn customers_connection_applies_name_sort_and_reverse_before_windowing() {
    let mut proxy = snapshot_proxy();
    create_customer(
        &mut proxy,
        "zulu-customer@example.test",
        "Zulu",
        "Customer",
        vec![],
        None,
    );
    create_customer(
        &mut proxy,
        "alpha-customer@example.test",
        "Alpha",
        "Customer",
        vec![],
        None,
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CustomersNameSort {
          ascending: customers(first: 10, sortKey: NAME) { nodes { email displayName } }
          descending: customers(first: 10, sortKey: NAME, reverse: true) { nodes { email displayName } }
        }
        "#,
        json!({}),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["ascending"]["nodes"],
        json!([
            { "email": "alpha-customer@example.test", "displayName": "Alpha Customer" },
            { "email": "zulu-customer@example.test", "displayName": "Zulu Customer" }
        ])
    );
    assert_eq!(
        read.body["data"]["descending"]["nodes"],
        json!([
            { "email": "zulu-customer@example.test", "displayName": "Zulu Customer" },
            { "email": "alpha-customer@example.test", "displayName": "Alpha Customer" }
        ])
    );
}

#[test]
fn customers_connection_applies_id_and_location_sort_keys() {
    let mut proxy = snapshot_proxy();
    create_customer_from_input(
        &mut proxy,
        json!({
            "email": "toronto-sort@example.test",
            "firstName": "Toronto",
            "lastName": "Sort",
            "addresses": [{
                "address1": "1 King St W",
                "city": "Toronto",
                "provinceCode": "ON",
                "countryCode": "CA",
                "zip": "M5H 1A1"
            }]
        }),
    );
    create_customer_from_input(
        &mut proxy,
        json!({
            "email": "ottawa-sort@example.test",
            "firstName": "Ottawa",
            "lastName": "Sort",
            "addresses": [{
                "address1": "111 Wellington St",
                "city": "Ottawa",
                "provinceCode": "ON",
                "countryCode": "CA",
                "zip": "K1A 0A4"
            }]
        }),
    );
    create_customer_from_input(
        &mut proxy,
        json!({
            "email": "seattle-sort@example.test",
            "firstName": "Seattle",
            "lastName": "Sort",
            "addresses": [{
                "address1": "600 4th Ave",
                "city": "Seattle",
                "provinceCode": "WA",
                "countryCode": "US",
                "zip": "98104"
            }]
        }),
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CustomersIdAndLocationSort {
          idOrder: customers(first: 5, sortKey: ID) {
            nodes { email }
          }
          idReverse: customers(first: 5, sortKey: ID, reverse: true) {
            nodes { email }
          }
          locationOrder: customers(first: 5, sortKey: LOCATION) {
            nodes { email defaultAddress { country province city } }
          }
          locationReverse: customers(first: 5, sortKey: LOCATION, reverse: true) {
            nodes { email defaultAddress { country province city } }
          }
        }
        "#,
        json!({}),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["idOrder"]["nodes"],
        json!([
            { "email": "toronto-sort@example.test" },
            { "email": "ottawa-sort@example.test" },
            { "email": "seattle-sort@example.test" }
        ])
    );
    assert_eq!(
        read.body["data"]["idReverse"]["nodes"],
        json!([
            { "email": "seattle-sort@example.test" },
            { "email": "ottawa-sort@example.test" },
            { "email": "toronto-sort@example.test" }
        ])
    );
    assert_eq!(
        read.body["data"]["locationOrder"]["nodes"],
        json!([
            {
                "email": "ottawa-sort@example.test",
                "defaultAddress": { "country": "Canada", "province": "Ontario", "city": "Ottawa" }
            },
            {
                "email": "toronto-sort@example.test",
                "defaultAddress": { "country": "Canada", "province": "Ontario", "city": "Toronto" }
            },
            {
                "email": "seattle-sort@example.test",
                "defaultAddress": { "country": "United States", "province": "Washington", "city": "Seattle" }
            }
        ])
    );
    assert_eq!(
        read.body["data"]["locationReverse"]["nodes"],
        json!([
            {
                "email": "seattle-sort@example.test",
                "defaultAddress": { "country": "United States", "province": "Washington", "city": "Seattle" }
            },
            {
                "email": "toronto-sort@example.test",
                "defaultAddress": { "country": "Canada", "province": "Ontario", "city": "Toronto" }
            },
            {
                "email": "ottawa-sort@example.test",
                "defaultAddress": { "country": "Canada", "province": "Ontario", "city": "Ottawa" }
            }
        ])
    );
}

#[test]
fn customers_query_filters_by_default_address_country() {
    let mut proxy = snapshot_proxy();
    create_customer_from_input(
        &mut proxy,
        json!({
            "email": "toronto-country@example.test",
            "firstName": "Toronto",
            "lastName": "Country",
            "tags": ["VIP"],
            "addresses": [{
                "address1": "1 King St W",
                "city": "Toronto",
                "provinceCode": "ON",
                "countryCode": "CA",
                "zip": "M5H 1A1"
            }]
        }),
    );
    create_customer_from_input(
        &mut proxy,
        json!({
            "email": "seattle-country@example.test",
            "firstName": "Seattle",
            "lastName": "Country",
            "tags": ["standard"],
            "addresses": [{
                "address1": "600 4th Ave",
                "city": "Seattle",
                "provinceCode": "WA",
                "countryCode": "US",
                "zip": "98104"
            }]
        }),
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CustomersSearchFields(
          $countryQuery: String!
          $stateQuery: String!
          $defaultQuery: String!
          $orQuery: String!
          $exclusionQuery: String!
          $unsupportedQuery: String!
        ) {
          byCountry: customers(first: 10, query: $countryQuery, sortKey: NAME) {
            nodes { email defaultAddress { country province city } }
            pageInfo { hasNextPage hasPreviousPage }
          }
          countryCount: customersCount(query: $countryQuery) { count precision }
          byState: customers(first: 10, query: $stateQuery, sortKey: NAME) {
            nodes { email state }
          }
          byDefault: customers(first: 10, query: $defaultQuery, sortKey: NAME) {
            nodes { email }
          }
          byGroupedOr: customers(first: 10, query: $orQuery, sortKey: NAME) {
            nodes { email tags }
          }
          byGroupedExclusion: customers(first: 10, query: $exclusionQuery, sortKey: NAME) {
            nodes { email tags }
          }
          byUnsupported: customers(first: 10, query: $unsupportedQuery, sortKey: NAME) {
            nodes { email }
          }
          unsupportedCount: customersCount(query: $unsupportedQuery) { count precision }
        }
        "#,
        json!({
            "countryQuery": "country:Canada",
            "stateQuery": "state:DISABLED",
            "defaultQuery": "Toronto",
            "orQuery": "(tag:VIP OR tag:standard) state:DISABLED",
            "exclusionQuery": "state:DISABLED -tag:VIP",
            "unsupportedQuery": "made_up_filter:Canada"
        }),
    ));

    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["byCountry"]["nodes"],
        json!([{
            "email": "toronto-country@example.test",
            "defaultAddress": { "country": "Canada", "province": "Ontario", "city": "Toronto" }
        }])
    );
    assert_eq!(
        read.body["data"]["byCountry"]["pageInfo"],
        json!({ "hasNextPage": false, "hasPreviousPage": false })
    );
    assert_eq!(
        read.body["data"]["countryCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["byState"]["nodes"],
        json!([
            { "email": "seattle-country@example.test", "state": "DISABLED" },
            { "email": "toronto-country@example.test", "state": "DISABLED" }
        ])
    );
    assert_eq!(
        read.body["data"]["byDefault"]["nodes"],
        json!([{ "email": "toronto-country@example.test" }])
    );
    assert_eq!(
        read.body["data"]["byGroupedOr"]["nodes"],
        json!([
            { "email": "seattle-country@example.test", "tags": ["standard"] },
            { "email": "toronto-country@example.test", "tags": ["VIP"] }
        ])
    );
    assert_eq!(
        read.body["data"]["byGroupedExclusion"]["nodes"],
        json!([{ "email": "seattle-country@example.test", "tags": ["standard"] }])
    );
    assert_eq!(
        read.body["data"]["byUnsupported"]["nodes"],
        json!([
            { "email": "seattle-country@example.test" },
            { "email": "toronto-country@example.test" }
        ])
    );
    assert_eq!(
        read.body["data"]["unsupportedCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
}

#[test]
fn customers_sorted_connection_paginates_after_interleaved_create() {
    let mut proxy = snapshot_proxy();
    create_customer(
        &mut proxy,
        "alpha-page@example.test",
        "Alpha",
        "Page",
        vec![],
        None,
    );
    create_customer(
        &mut proxy,
        "zulu-page@example.test",
        "Zulu",
        "Page",
        vec![],
        None,
    );

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query CustomersNameFirstPage {
          customers(first: 1, sortKey: NAME) {
            edges { cursor node { email displayName } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        first_page.body["data"]["customers"]["edges"][0]["node"],
        json!({ "email": "alpha-page@example.test", "displayName": "Alpha Page" })
    );

    create_customer(
        &mut proxy,
        "aaron-page@example.test",
        "Aaron",
        "Page",
        vec![],
        None,
    );

    let next_page = proxy.process_request(json_graphql_request(
        r#"
        query CustomersNameNextPage($after: String!) {
          customers(first: 1, after: $after, sortKey: NAME) {
            nodes { email displayName }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"after": first_page.body["data"]["customers"]["pageInfo"]["endCursor"]}),
    ));
    assert_eq!(
        next_page.body["data"]["customers"]["nodes"],
        json!([{ "email": "zulu-page@example.test", "displayName": "Zulu Page" }])
    );
    assert_eq!(
        next_page.body["data"]["customers"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );

    let before_page = proxy.process_request(json_graphql_request(
        r#"
        query CustomersNameBeforePage($before: String!) {
          customers(last: 1, before: $before, sortKey: NAME) {
            nodes { email displayName }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({"before": next_page.body["data"]["customers"]["pageInfo"]["startCursor"]}),
    ));
    assert_eq!(
        before_page.body["data"]["customers"]["nodes"],
        json!([{ "email": "alpha-page@example.test", "displayName": "Alpha Page" }])
    );
}

#[test]
fn customers_filtered_sorted_connection_counts_and_reverses_after_interleaved_update() {
    let mut proxy = snapshot_proxy();
    create_customer(
        &mut proxy,
        "beta-filtered@example.test",
        "Beta",
        "Shopper",
        vec!["vip".to_string()],
        None,
    );
    create_customer(
        &mut proxy,
        "zulu-filtered@example.test",
        "Zulu",
        "Shopper",
        vec!["vip".to_string()],
        None,
    );
    let alpha_id = create_customer(
        &mut proxy,
        "alpha-filtered@example.test",
        "Alpha",
        "Shopper",
        vec!["standard".to_string()],
        None,
    );

    let first_page = proxy.process_request(json_graphql_request(
        r#"
        query CustomersFilteredFirstPage($query: String!) {
          customers(first: 1, query: $query, sortKey: NAME) {
            edges { cursor node { email displayName tags } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "query": "tag:vip" }),
    ));
    assert_eq!(
        first_page.body["data"]["customers"]["edges"][0]["node"],
        json!({
            "email": "beta-filtered@example.test",
            "displayName": "Beta Shopper",
            "tags": ["vip"]
        })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation PromoteAlphaCustomer($input: CustomerInput!) {
          customerUpdate(input: $input) {
            customer { id tags }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "id": alpha_id,
                "tags": ["vip", "standard"]
            }
        }),
    ));
    assert_eq!(
        update.body["data"]["customerUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["customerUpdate"]["customer"]["tags"],
        json!(["standard", "vip"])
    );

    let after_page = proxy.process_request(json_graphql_request(
        r#"
        query CustomersFilteredAfterPage($query: String!, $after: String!) {
          customers(first: 1, after: $after, query: $query, sortKey: NAME) {
            nodes { email displayName tags }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({
            "query": "tag:vip",
            "after": first_page.body["data"]["customers"]["pageInfo"]["endCursor"]
        }),
    ));
    assert_eq!(
        after_page.body["data"]["customers"]["nodes"],
        json!([{
            "email": "zulu-filtered@example.test",
            "displayName": "Zulu Shopper",
            "tags": ["vip"]
        }])
    );
    assert_eq!(
        after_page.body["data"]["customers"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );

    let read_all = proxy.process_request(json_graphql_request(
        r#"
        query CustomersFilteredAllAndCount($query: String!) {
          customers(first: 10, query: $query, sortKey: NAME) {
            nodes { email displayName tags }
            pageInfo { hasNextPage hasPreviousPage }
          }
          customersCount(query: $query) { count precision }
        }
        "#,
        json!({ "query": "tag:vip" }),
    ));
    assert_eq!(
        read_all.body["data"]["customers"]["nodes"],
        json!([
            {
                "email": "alpha-filtered@example.test",
                "displayName": "Alpha Shopper",
                "tags": ["standard", "vip"]
            },
            {
                "email": "beta-filtered@example.test",
                "displayName": "Beta Shopper",
                "tags": ["vip"]
            },
            {
                "email": "zulu-filtered@example.test",
                "displayName": "Zulu Shopper",
                "tags": ["vip"]
            }
        ])
    );
    assert_eq!(
        read_all.body["data"]["customersCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );

    let reverse_first = proxy.process_request(json_graphql_request(
        r#"
        query CustomersFilteredReverseFirst($query: String!) {
          customers(first: 1, query: $query, sortKey: NAME, reverse: true) {
            edges { cursor node { email displayName } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "query": "tag:vip" }),
    ));
    assert_eq!(
        reverse_first.body["data"]["customers"]["edges"][0]["node"],
        json!({
            "email": "zulu-filtered@example.test",
            "displayName": "Zulu Shopper"
        })
    );

    let reverse_after = proxy.process_request(json_graphql_request(
        r#"
        query CustomersFilteredReverseAfter($query: String!, $after: String!) {
          customers(first: 1, after: $after, query: $query, sortKey: NAME, reverse: true) {
            nodes { email displayName }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({
            "query": "tag:vip",
            "after": reverse_first.body["data"]["customers"]["pageInfo"]["endCursor"]
        }),
    ));
    assert_eq!(
        reverse_after.body["data"]["customers"]["nodes"],
        json!([{
            "email": "beta-filtered@example.test",
            "displayName": "Beta Shopper"
        }])
    );
    assert_eq!(
        reverse_after.body["data"]["customers"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(
        reverse_after.body["data"]["customers"]["pageInfo"]["hasPreviousPage"],
        json!(true)
    );
}

#[test]
fn customer_merge_stages_and_downstream_reads_are_operation_name_independent() {
    let mut proxy = snapshot_proxy();
    let source_id = create_customer_from_input(
        &mut proxy,
        json!({
            "email": "merge-source@example.test",
            "firstName": "Merge",
            "lastName": "Source",
            "tags": ["source"],
            "note": "source note",
            "metafields": [
                { "namespace": "custom", "key": "source_only", "type": "single_line_text_field", "value": "source" }
            ]
        }),
    );
    let result_id = create_customer_from_input(
        &mut proxy,
        json!({
            "email": "merge-result@example.test",
            "firstName": "Merge",
            "lastName": "Result",
            "tags": ["result"],
            "metafields": [
                { "namespace": "custom", "key": "result_only", "type": "single_line_text_field", "value": "result" }
            ]
        }),
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
          result: customer(id: $result) {
            id
            email
            firstName
            lastName
            displayName
            note
            tags
            defaultEmailAddress { emailAddress }
            metafields(first: 5) {
              nodes { namespace key type value }
              pageInfo { hasNextPage hasPreviousPage }
            }
            metafieldsReverse: metafields(first: 1, reverse: true) {
              nodes { namespace key type value }
              pageInfo { hasNextPage hasPreviousPage }
            }
          }
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
            "defaultEmailAddress": { "emailAddress": "merge-result@example.test" },
            "metafields": {
                "nodes": [
                    { "namespace": "custom", "key": "result_only", "type": "single_line_text_field", "value": "result" },
                    { "namespace": "custom", "key": "source_only", "type": "single_line_text_field", "value": "source" }
                ],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false
                }
            },
            "metafieldsReverse": {
                "nodes": [
                    { "namespace": "custom", "key": "source_only", "type": "single_line_text_field", "value": "source" }
                ],
                "pageInfo": {
                    "hasNextPage": true,
                    "hasPreviousPage": false
                }
            }
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
fn customer_detail_connections_apply_query_sort_reverse_and_page_info() {
    fn create_customer_order(
        proxy: &mut DraftProxy,
        customer_id: &str,
        email: &str,
        tag: &str,
        title: &str,
        processed_at: &str,
    ) -> String {
        let response = proxy.process_request(json_graphql_request(
            r#"
            mutation CreateCustomerDetailOrder($order: OrderCreateOrderInput!) {
              orderCreate(order: $order) {
                order { id email tags processedAt createdAt updatedAt }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "order": {
                    "email": email,
                    "currency": "USD",
                    "financialStatus": "PENDING",
                    "processedAt": processed_at,
                    "tags": [tag],
                    "lineItems": [{
                        "title": title,
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": "10.00", "currencyCode": "USD" } }
                    }]
                }
            }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["orderCreate"]["userErrors"],
            json!([])
        );
        let order_id = response.body["data"]["orderCreate"]["order"]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let attach = proxy.process_request(json_graphql_request(
            r#"
            mutation AttachCustomerDetailOrder($orderId: ID!, $customerId: ID!) {
              orderCustomerSet(orderId: $orderId, customerId: $customerId) {
                order { id email tags processedAt customer { id } }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "orderId": order_id,
                "customerId": customer_id
            }),
        ));
        assert_eq!(attach.status, 200);
        assert_eq!(
            attach.body["data"]["orderCustomerSet"]["userErrors"],
            json!([])
        );
        order_id
    }

    let mut proxy = snapshot_proxy();
    let customer_id = create_customer_from_input(
        &mut proxy,
        json!({
            "email": "customer-detail-connections@example.test",
            "firstName": "Connection",
            "lastName": "Subject",
            "addresses": [
                { "address1": "1 First St", "city": "Alpha", "countryCode": "US", "provinceCode": "NY", "zip": "10001" },
                { "address1": "2 Second St", "city": "Beta", "countryCode": "US", "provinceCode": "CA", "zip": "90001" }
            ],
            "metafields": [
                { "namespace": "custom", "key": "alpha", "type": "single_line_text_field", "value": "one" },
                { "namespace": "custom", "key": "beta", "type": "single_line_text_field", "value": "two" }
            ]
        }),
    );
    create_customer_order(
        &mut proxy,
        &customer_id,
        "standard-order@example.test",
        "standard",
        "Standard detail order",
        "2024-01-01T00:00:00Z",
    );
    create_customer_order(
        &mut proxy,
        &customer_id,
        "old-vip-order@example.test",
        "vip",
        "Old VIP detail order",
        "2024-02-01T00:00:00Z",
    );
    let newest_vip_id = create_customer_order(
        &mut proxy,
        &customer_id,
        "new-vip-order@example.test",
        "vip",
        "New VIP detail order",
        "2024-03-01T00:00:00Z",
    );

    for (amount, currency) in [("1.00", "USD"), ("2.00", "EUR")] {
        let response = proxy.process_request(json_graphql_request(
            r#"
            mutation CreditCustomerStoreCredit($customerId: ID!, $amount: MoneyInput!) {
              storeCreditAccountCredit(id: $customerId, creditInput: { creditAmount: $amount }) {
                storeCreditAccountTransaction { account { id balance { amount currencyCode } } }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "customerId": customer_id,
                "amount": { "amount": amount, "currencyCode": currency }
            }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["storeCreditAccountCredit"]["userErrors"],
            json!([])
        );
    }

    let read = proxy.process_request(json_graphql_request(
        r#"
        query CustomerDetailConnectionArgs($id: ID!) {
          customer(id: $id) {
            orders(first: 1, query: "processed_at:>=2024-02-01", sortKey: PROCESSED_AT, reverse: true) {
              nodes { id email tags processedAt }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            storeCreditAccounts(first: 5, query: "currency_code:EUR") {
              nodes { id balance { amount currencyCode } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            addressesV2(first: 1, reverse: true) {
              nodes { address1 city }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            metafields(first: 1, reverse: true) {
              nodes { namespace key value }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({ "id": customer_id }),
    ));
    assert_eq!(read.status, 200);

    let customer = &read.body["data"]["customer"];
    assert_eq!(
        customer["orders"]["nodes"],
        json!([{
            "id": newest_vip_id,
            "email": "new-vip-order@example.test",
            "tags": ["vip"],
            "processedAt": "2024-03-01T00:00:00Z"
        }])
    );
    assert_eq!(customer["orders"]["pageInfo"]["hasNextPage"], json!(true));
    assert_eq!(
        customer["orders"]["pageInfo"]["hasPreviousPage"],
        json!(false)
    );

    assert_eq!(
        customer["storeCreditAccounts"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        customer["storeCreditAccounts"]["nodes"][0]["balance"]["currencyCode"],
        json!("EUR")
    );
    assert_eq!(
        customer["storeCreditAccounts"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": customer["storeCreditAccounts"]["nodes"][0]["id"].clone(),
            "endCursor": customer["storeCreditAccounts"]["nodes"][0]["id"].clone()
        })
    );

    assert_eq!(
        customer["addressesV2"]["nodes"],
        json!([{ "address1": "2 Second St", "city": "Beta" }])
    );
    assert_eq!(
        customer["addressesV2"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(
        customer["addressesV2"]["pageInfo"]["hasPreviousPage"],
        json!(false)
    );
    assert!(customer["addressesV2"]["pageInfo"]["startCursor"].is_string());

    assert_eq!(
        customer["metafields"]["nodes"],
        json!([{ "namespace": "custom", "key": "beta", "value": "two" }])
    );
    assert_eq!(
        customer["metafields"]["pageInfo"]["hasNextPage"],
        json!(true)
    );
    assert_eq!(
        customer["metafields"]["pageInfo"]["hasPreviousPage"],
        json!(false)
    );
    assert!(customer["metafields"]["pageInfo"]["startCursor"].is_string());
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
fn customer_address_phone_normalizes_international_format_without_inferring_country() {
    let mut proxy = snapshot_proxy();
    let customer_id = create_customer(
        &mut proxy,
        "address-phone-normalization@example.test",
        "Address",
        "Phone",
        Vec::new(),
        None,
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CreateAddressPhone($customerId: ID!, $address: MailingAddressInput!) {
          customerAddressCreate(customerId: $customerId, address: $address, setAsDefault: true) {
            address { id phone }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "customerId": customer_id,
            "address": {
                "address1": "1 Normalized Way",
                "city": "Ottawa",
                "countryCode": "CA",
                "phone": "+1 (613) 450-4538"
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["customerAddressCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["customerAddressCreate"]["address"]["phone"],
        json!("+16134504538")
    );
    let address_id = create.body["data"]["customerAddressCreate"]["address"]["id"]
        .as_str()
        .expect("address id")
        .to_string();

    let downstream = proxy.process_request(json_graphql_request(
        r#"
        query AddressPhoneReadback($id: ID!) {
          customer(id: $id) {
            defaultAddress { phone }
            addressesV2(first: 5) { nodes { id phone } }
          }
        }
        "#,
        json!({ "id": customer_id }),
    ));
    assert_eq!(
        downstream.body["data"]["customer"]["defaultAddress"]["phone"],
        json!("+16134504538")
    );
    assert_eq!(
        downstream.body["data"]["customer"]["addressesV2"]["nodes"][0]["phone"],
        json!("+16134504538")
    );

    let update_formatted = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateAddressPhone($customerId: ID!, $addressId: ID!, $address: MailingAddressInput!) {
          customerAddressUpdate(
            customerId: $customerId
            addressId: $addressId
            address: $address
            setAsDefault: true
          ) {
            address { id phone }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "customerId": customer_id,
            "addressId": address_id,
            "address": { "phone": "+1-613-450-4538" }
        }),
    ));
    assert_eq!(
        update_formatted.body["data"]["customerAddressUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update_formatted.body["data"]["customerAddressUpdate"]["address"]["phone"],
        json!("+16134504538")
    );

    let update_local = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateAddressLocalPhone($customerId: ID!, $addressId: ID!, $address: MailingAddressInput!) {
          customerAddressUpdate(
            customerId: $customerId
            addressId: $addressId
            address: $address
            setAsDefault: true
          ) {
            address { id phone }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "customerId": customer_id,
            "addressId": address_id,
            "address": { "phone": "450-4538" }
        }),
    ));
    assert_eq!(
        update_local.body["data"]["customerAddressUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update_local.body["data"]["customerAddressUpdate"]["address"]["phone"],
        json!("+14504538")
    );

    let local_downstream = proxy.process_request(json_graphql_request(
        r#"
        query LocalAddressPhoneReadback($id: ID!) {
          customer(id: $id) {
            defaultAddress { phone }
            addressesV2(first: 5) { nodes { id phone } }
          }
        }
        "#,
        json!({ "id": customer_id }),
    ));
    assert_eq!(
        local_downstream.body["data"]["customer"]["defaultAddress"]["phone"],
        json!("+14504538")
    );
    assert_eq!(
        local_downstream.body["data"]["customer"]["addressesV2"]["nodes"][0]["phone"],
        json!("+14504538")
    );

    let update_country_local = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateAddressCountryLocalPhone($customerId: ID!, $addressId: ID!, $address: MailingAddressInput!) {
          customerAddressUpdate(
            customerId: $customerId
            addressId: $addressId
            address: $address
            setAsDefault: true
          ) {
            address { id phone countryCodeV2 }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "customerId": customer_id,
            "addressId": address_id,
            "address": { "countryCode": "DK", "phone": "12345678" }
        }),
    ));
    assert_eq!(
        update_country_local.body["data"]["customerAddressUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update_country_local.body["data"]["customerAddressUpdate"]["address"]["phone"],
        json!("+4512345678")
    );

    let update_raw = proxy.process_request(json_graphql_request(
        r#"
        mutation UpdateAddressRawPhone($customerId: ID!, $addressId: ID!, $address: MailingAddressInput!) {
          customerAddressUpdate(
            customerId: $customerId
            addressId: $addressId
            address: $address
            setAsDefault: true
          ) {
            address { id phone }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "customerId": customer_id,
            "addressId": address_id,
            "address": { "phone": "not a phone" }
        }),
    ));
    assert_eq!(
        update_raw.body["data"]["customerAddressUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update_raw.body["data"]["customerAddressUpdate"]["address"]["phone"],
        json!("not a phone")
    );

    let raw_downstream = proxy.process_request(json_graphql_request(
        r#"
        query RawAddressPhoneReadback($id: ID!) {
          customer(id: $id) {
            defaultAddress { phone }
            addressesV2(first: 5) { nodes { id phone } }
          }
        }
        "#,
        json!({ "id": customer_id }),
    ));
    assert_eq!(
        raw_downstream.body["data"]["customer"]["defaultAddress"]["phone"],
        json!("not a phone")
    );
    assert_eq!(
        raw_downstream.body["data"]["customer"]["addressesV2"]["nodes"][0]["phone"],
        json!("not a phone")
    );
}

#[test]
fn customer_phone_uses_restored_shop_country_for_bare_numbers() {
    let mut proxy = snapshot_proxy();
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({
            "id": "gid://shopify/Shop/danish-customer-phone",
            "shopAddress": {
                "countryCodeV2": "DK",
                "countryCode": "DK"
            }
        });
    });

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation CustomerPhoneCountryContext($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id phone defaultPhoneNumber { phoneNumber } }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "country-phone@example.test",
                "phone": "12345678"
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["customerCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create.body["data"]["customerCreate"]["customer"]["phone"],
        json!("+4512345678")
    );
    assert_eq!(
        create.body["data"]["customerCreate"]["customer"]["defaultPhoneNumber"]["phoneNumber"],
        json!("+4512345678")
    );
}

#[test]
fn customer_address_mutations_report_missing_customer_before_address_lookup() {
    let mut proxy = snapshot_proxy();
    let existing_customer_id = create_customer(
        &mut proxy,
        "address-owner@example.test",
        "Address",
        "Owner",
        Vec::new(),
        None,
    );
    let foreign_address_id =
        create_customer_address(&mut proxy, &existing_customer_id, "1 Foreign Address Rd");
    let missing_customer_id = "gid://shopify/Customer/999999999999999";
    let unknown_address_id = "gid://shopify/MailingAddress/999999999999999";
    let assert_resource_not_found = |response: &Response, root: &str| {
        assert_eq!(response.status, 200);
        assert_eq!(response.body["data"][root], Value::Null);
        assert_eq!(
            response.body["errors"][0]["extensions"]["code"],
            json!("RESOURCE_NOT_FOUND")
        );
        assert_eq!(response.body["errors"][0]["path"], json!([root]));
    };

    for (address_id, expect_customer_error) in [
        (unknown_address_id, false),
        (foreign_address_id.as_str(), true),
    ] {
        let update = proxy.process_request(json_graphql_request(
            r#"
            mutation MissingCustomerAddressUpdate(
              $customerId: ID!
              $addressId: ID!
              $address: MailingAddressInput!
            ) {
              customerAddressUpdate(
                customerId: $customerId
                addressId: $addressId
                address: $address
              ) {
                address { id }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "customerId": missing_customer_id,
                "addressId": address_id,
                "address": { "address1": "Updated" }
            }),
        ));
        if expect_customer_error {
            assert_eq!(update.status, 200);
            assert!(update.body.get("errors").is_none());
            assert_eq!(
                update.body["data"]["customerAddressUpdate"],
                json!({
                    "address": null,
                    "userErrors": [{
                        "field": ["customerId"],
                        "message": "Customer does not exist"
                    }]
                })
            );
        } else {
            assert_resource_not_found(&update, "customerAddressUpdate");
        }

        let delete = proxy.process_request(json_graphql_request(
            r#"
            mutation MissingCustomerAddressDelete($customerId: ID!, $addressId: ID!) {
              customerAddressDelete(customerId: $customerId, addressId: $addressId) {
                deletedAddressId
                userErrors { field message }
              }
            }
            "#,
            json!({
                "customerId": missing_customer_id,
                "addressId": address_id
            }),
        ));
        if expect_customer_error {
            assert_eq!(delete.status, 200);
            assert!(delete.body.get("errors").is_none());
            assert_eq!(
                delete.body["data"]["customerAddressDelete"],
                json!({
                    "deletedAddressId": null,
                    "userErrors": [{
                        "field": ["customerId"],
                        "message": "Customer does not exist"
                    }]
                })
            );
        } else {
            assert_resource_not_found(&delete, "customerAddressDelete");
        }

        let default_address = proxy.process_request(json_graphql_request(
            r#"
            mutation MissingCustomerDefaultAddress($customerId: ID!, $addressId: ID!) {
              customerUpdateDefaultAddress(customerId: $customerId, addressId: $addressId) {
                customer { id }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "customerId": missing_customer_id,
                "addressId": address_id
            }),
        ));
        if expect_customer_error {
            assert_eq!(default_address.status, 200);
            assert!(default_address.body.get("errors").is_none());
            assert_eq!(
                default_address.body["data"]["customerUpdateDefaultAddress"],
                json!({
                    "customer": null,
                    "userErrors": [{
                        "field": ["customerId"],
                        "message": "Customer does not exist"
                    }]
                })
            );
        } else {
            assert_resource_not_found(&default_address, "customerUpdateDefaultAddress");
        }
    }
}

#[test]
fn customer_address_mutations_keep_address_error_when_customer_exists() {
    let mut proxy = snapshot_proxy();
    let target_customer_id = create_customer(
        &mut proxy,
        "address-target@example.test",
        "Address",
        "Target",
        Vec::new(),
        None,
    );
    let foreign_customer_id = create_customer(
        &mut proxy,
        "address-foreign@example.test",
        "Address",
        "Foreign",
        Vec::new(),
        None,
    );
    let foreign_address_id =
        create_customer_address(&mut proxy, &foreign_customer_id, "2 Foreign Address Rd");
    let expected_error = json!([{
        "field": ["addressId"],
        "message": "Address does not exist"
    }]);

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation ExistingCustomerAddressUpdate(
          $customerId: ID!
          $addressId: ID!
          $address: MailingAddressInput!
        ) {
          customerAddressUpdate(
            customerId: $customerId
            addressId: $addressId
            address: $address
          ) {
            address { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "customerId": target_customer_id,
            "addressId": foreign_address_id,
            "address": { "address1": "Updated" }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["customerAddressUpdate"]["userErrors"],
        expected_error
    );
    assert_eq!(
        update.body["data"]["customerAddressUpdate"]["address"],
        Value::Null
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation ExistingCustomerAddressDelete($customerId: ID!, $addressId: ID!) {
          customerAddressDelete(customerId: $customerId, addressId: $addressId) {
            deletedAddressId
            userErrors { field message }
          }
        }
        "#,
        json!({
            "customerId": target_customer_id,
            "addressId": foreign_address_id
        }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["customerAddressDelete"]["userErrors"],
        expected_error
    );
    assert_eq!(
        delete.body["data"]["customerAddressDelete"]["deletedAddressId"],
        Value::Null
    );

    let default_address = proxy.process_request(json_graphql_request(
        r#"
        mutation ExistingCustomerDefaultAddress($customerId: ID!, $addressId: ID!) {
          customerUpdateDefaultAddress(customerId: $customerId, addressId: $addressId) {
            customer { id }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "customerId": target_customer_id,
            "addressId": foreign_address_id
        }),
    ));
    assert_eq!(default_address.status, 200);
    assert_eq!(
        default_address.body["data"]["customerUpdateDefaultAddress"]["userErrors"],
        expected_error
    );
    assert_eq!(
        default_address.body["data"]["customerUpdateDefaultAddress"]["customer"]["id"],
        json!(target_customer_id)
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
