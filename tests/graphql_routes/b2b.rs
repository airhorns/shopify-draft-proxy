use super::common::*;
use pretty_assertions::assert_eq;

#[test]
fn b2b_tax_settings_update_tail_helpers_cover_current_behavior() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "Tax Settings Co");
    let location_id = create_b2b_location(&mut proxy, &company_id, "Tax Settings Branch");

    let required_and_nullable = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BTaxSettingsRequiredNullable($locationId: ID!, $taxExempt: Boolean) {
          emptyInput: companyLocationTaxSettingsUpdate(companyLocationId: $locationId) {
            companyLocation { id taxSettings { taxExempt taxExemptions } }
            userErrors { field message code }
          }
          nullTaxExempt: companyLocationTaxSettingsUpdate(companyLocationId: $locationId, taxExempt: $taxExempt) {
            companyLocation { id taxSettings { taxExempt } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id, "taxExempt": Value::Null }),
    ));
    assert_eq!(required_and_nullable.status, 200);
    assert_eq!(
        required_and_nullable.body["data"]["emptyInput"],
        json!({
            "companyLocation": {
                "id": location_id,
                "taxSettings": {
                    "taxExempt": false,
                    "taxExemptions": []
                }
            },
            "userErrors": []
        })
    );
    assert_eq!(
        required_and_nullable.body["data"]["nullTaxExempt"],
        json!({
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": ["taxExempt"],
                "message": "Tax exempt must be true or false",
                "code": "INVALID_INPUT"
            }]
        })
    );

    let invalid_literal = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BTaxSettingsInvalidEnumLiteral {
          companyLocationTaxSettingsUpdate(companyLocationId: "gid://shopify/CompanyLocation/123", exemptionsToAssign: [FOO_BAR]) {
            companyLocation { id taxSettings { taxExemptions } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(invalid_literal.status, 200);
    assert_eq!(
        invalid_literal.body["errors"][0]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );
    assert_eq!(
        invalid_literal.body["errors"][0]["extensions"]["typeName"],
        json!("Field")
    );
    assert_eq!(
        invalid_literal.body["errors"][0]["extensions"]["argumentName"],
        json!("exemptionsToAssign")
    );
    assert_eq!(
        invalid_literal.body["errors"][0]["message"],
        json!("Argument 'exemptionsToAssign' on Field 'companyLocationTaxSettingsUpdate' has an invalid value ([FOO_BAR]). Expected type '[TaxExemption!]'.")
    );
    assert_eq!(
        invalid_literal.body["errors"][0]["locations"],
        json!([{ "line": 3, "column": 11 }])
    );
    assert_eq!(
        invalid_literal.body["errors"][0]["path"],
        json!([
            "mutation RustB2BTaxSettingsInvalidEnumLiteral",
            "companyLocationTaxSettingsUpdate",
            "exemptionsToAssign"
        ])
    );
    assert!(!invalid_literal.body["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("Did you mean"));
    assert!(!invalid_literal.body["errors"][0]["message"]
        .as_str()
        .unwrap()
        .contains("NOT_A_REAL_EXEMPTION"));
    assert!(invalid_literal.body["data"].is_null());

    let invalid_variable = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BTaxSettingsInvalidEnumVariable($locationId: ID!, $exemptionsToAssign: [TaxExemption!]) {
          companyLocationTaxSettingsUpdate(companyLocationId: $locationId, exemptionsToAssign: $exemptionsToAssign) {
            companyLocation { id taxSettings { taxExemptions } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": location_id,
            "exemptionsToAssign": ["NOT_A_REAL_EXEMPTION"]
        }),
    ));
    assert_eq!(invalid_variable.status, 200);
    assert_eq!(
        invalid_variable.body["errors"][0]["extensions"]["code"],
        json!("INVALID_VARIABLE")
    );
    assert!(invalid_variable.body["errors"][0]["message"]
        .as_str()
        .is_some_and(|message| message.contains("NOT_A_REAL_EXEMPTION")
            && message.contains("CA_STATUS_CARD_EXEMPTION")));

    let assign = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BTaxSettingsAssignRemove($locationId: ID!, $assign: [TaxExemption!]) {
          companyLocationTaxSettingsUpdate(companyLocationId: $locationId, exemptionsToAssign: $assign) {
            companyLocation { id taxSettings { taxExemptions } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": location_id,
            "assign": ["CA_BC_RESELLER_EXEMPTION", "US_CA_RESELLER_EXEMPTION"]
        }),
    ));
    assert_eq!(assign.status, 200);
    assert_eq!(
        assign.body["data"]["companyLocationTaxSettingsUpdate"],
        json!({
            "companyLocation": {
                "id": location_id,
                "taxSettings": {
                    "taxExemptions": ["CA_BC_RESELLER_EXEMPTION", "US_CA_RESELLER_EXEMPTION"]
                }
            },
            "userErrors": []
        })
    );

    let remove = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BTaxSettingsAssignRemove($locationId: ID!, $remove: [TaxExemption!]) {
          companyLocationTaxSettingsUpdate(companyLocationId: $locationId, exemptionsToRemove: $remove) {
            companyLocation { id taxSettings { taxExemptions } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": location_id,
            "remove": ["CA_BC_RESELLER_EXEMPTION"]
        }),
    ));
    assert_eq!(remove.status, 200);
    assert_eq!(
        remove.body["data"]["companyLocationTaxSettingsUpdate"]["companyLocation"]["taxSettings"]
            ["taxExemptions"],
        json!(["US_CA_RESELLER_EXEMPTION"])
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("log entries");
    assert!(entries
        .iter()
        .any(|entry| entry["status"] == json!("failed")
            && entry["interpreted"]["primaryRootField"]
                == json!("companyLocationTaxSettingsUpdate")));
    assert!(entries
        .iter()
        .any(|entry| entry["status"] == json!("staged")
            && entry["interpreted"]["primaryRootField"]
                == json!("companyLocationTaxSettingsUpdate")));
}

#[test]
fn b2b_tax_settings_update_rejects_unhydrated_synthetic_location_and_uses_actual_location_state() {
    let mut proxy = snapshot_proxy();
    let synthetic_location_id = "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic";

    let missing = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BTaxSettingsSyntheticLocationRejected($locationId: ID!) {
          companyLocationTaxSettingsUpdate(companyLocationId: $locationId) {
            companyLocation { id name taxSettings { taxExempt taxExemptions } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": synthetic_location_id }),
    ));
    assert_eq!(missing.status, 200);
    assert_eq!(
        missing.body["data"]["companyLocationTaxSettingsUpdate"],
        json!({
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": ["companyLocationId"],
                "message": "The company location doesn't exist",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let company_id = create_b2b_company(&mut proxy, "Taxable Co");
    let location_id = create_b2b_location(&mut proxy, &company_id, "Taxable Branch");
    let actual = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BTaxSettingsActualLocation($locationId: ID!) {
          companyLocationTaxSettingsUpdate(companyLocationId: $locationId) {
            companyLocation { id name taxSettings { taxExempt taxExemptions } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(actual.status, 200);
    assert_eq!(
        actual.body["data"]["companyLocationTaxSettingsUpdate"],
        json!({
            "companyLocation": {
                "id": location_id,
                "name": "Taxable Branch",
                "taxSettings": {
                    "taxExempt": false,
                    "taxExemptions": []
                }
            },
            "userErrors": []
        })
    );
}

#[test]
fn b2b_tax_settings_update_registration_only_and_no_knobs_are_successful() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "Registration Tax Co");
    let location_id = create_b2b_location(&mut proxy, &company_id, "Registration Branch");

    let no_knobs = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BTaxSettingsNoKnobs($locationId: ID!) {
          companyLocationTaxSettingsUpdate(companyLocationId: $locationId) {
            companyLocation {
              id
              taxSettings {
                taxRegistrationId
                taxExempt
                taxExemptions
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(no_knobs.status, 200);
    assert_eq!(
        no_knobs.body["data"]["companyLocationTaxSettingsUpdate"],
        json!({
            "companyLocation": {
                "id": location_id,
                "taxSettings": {
                    "taxRegistrationId": Value::Null,
                    "taxExempt": false,
                    "taxExemptions": []
                }
            },
            "userErrors": []
        })
    );

    let registration_only = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BTaxSettingsRegistrationOnly($locationId: ID!, $taxRegistrationId: String) {
          companyLocationTaxSettingsUpdate(
            companyLocationId: $locationId,
            taxRegistrationId: $taxRegistrationId
          ) {
            companyLocation {
              id
              taxSettings {
                taxRegistrationId
                taxExempt
                taxExemptions
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id, "taxRegistrationId": "VAT-123" }),
    ));
    assert_eq!(registration_only.status, 200);
    assert_eq!(
        registration_only.body["data"]["companyLocationTaxSettingsUpdate"],
        json!({
            "companyLocation": {
                "id": location_id,
                "taxSettings": {
                    "taxRegistrationId": "VAT-123",
                    "taxExempt": false,
                    "taxExemptions": []
                }
            },
            "userErrors": []
        })
    );

    let read_after_write = proxy.process_request(json_graphql_request(
        r#"
        query RustB2BTaxSettingsRegistrationRead($locationId: ID!) {
          companyLocation(id: $locationId) {
            id
            taxSettings {
              taxRegistrationId
              taxExempt
              taxExemptions
            }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(read_after_write.status, 200);
    assert_eq!(
        read_after_write.body["data"]["companyLocation"],
        json!({
            "id": location_id,
            "taxSettings": {
                "taxRegistrationId": "VAT-123",
                "taxExempt": false,
                "taxExemptions": []
            }
        })
    );

    let log = log_snapshot(&proxy);
    let staged_tax_updates = log["entries"]
        .as_array()
        .expect("log entries")
        .iter()
        .filter(|entry| {
            entry["status"] == json!("staged")
                && entry["interpreted"]["primaryRootField"]
                    == json!("companyLocationTaxSettingsUpdate")
        })
        .count();
    assert_eq!(staged_tax_updates, 2);
}

#[test]
fn b2b_tax_settings_update_merges_exemptions_and_preserves_omitted_tax_exempt() {
    let mut fresh_proxy = snapshot_proxy();
    let fresh_company_id = create_b2b_company(&mut fresh_proxy, "Fresh Tax Co");
    let fresh_location_id =
        create_b2b_location(&mut fresh_proxy, &fresh_company_id, "Fresh Tax Branch");
    let assign_and_remove = fresh_proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BTaxSettingsAssignAndRemove(
          $locationId: ID!,
          $assign: [TaxExemption!],
          $remove: [TaxExemption!]
        ) {
          companyLocationTaxSettingsUpdate(
            companyLocationId: $locationId,
            exemptionsToAssign: $assign,
            exemptionsToRemove: $remove
          ) {
            companyLocation { id taxSettings { taxExemptions } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": fresh_location_id,
            "assign": ["EU_REVERSE_CHARGE_EXEMPTION_RULE"],
            "remove": ["US_CA_RESELLER_EXEMPTION"]
        }),
    ));
    assert_eq!(assign_and_remove.status, 200);
    assert_eq!(
        assign_and_remove.body["data"]["companyLocationTaxSettingsUpdate"],
        json!({
            "companyLocation": {
                "id": fresh_location_id,
                "taxSettings": {
                    "taxExemptions": ["EU_REVERSE_CHARGE_EXEMPTION_RULE"]
                }
            },
            "userErrors": []
        })
    );

    let mut staged_proxy = snapshot_proxy();
    let staged_company_id = create_b2b_company(&mut staged_proxy, "Staged Tax Co");
    let staged_location_id =
        create_b2b_location(&mut staged_proxy, &staged_company_id, "Staged Tax Branch");
    let initial = staged_proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BTaxSettingsInitial(
          $locationId: ID!,
          $taxRegistrationId: String,
          $taxExempt: Boolean,
          $assign: [TaxExemption!]
        ) {
          companyLocationTaxSettingsUpdate(
            companyLocationId: $locationId,
            taxRegistrationId: $taxRegistrationId,
            taxExempt: $taxExempt,
            exemptionsToAssign: $assign
          ) {
            companyLocation {
              id
              taxSettings {
                taxRegistrationId
                taxExempt
                taxExemptions
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": staged_location_id,
            "taxRegistrationId": "REG-1",
            "taxExempt": true,
            "assign": ["EU_REVERSE_CHARGE_EXEMPTION_RULE"]
        }),
    ));
    assert_eq!(initial.status, 200);
    assert_eq!(
        initial.body["data"]["companyLocationTaxSettingsUpdate"]["companyLocation"]["taxSettings"],
        json!({
            "taxRegistrationId": "REG-1",
            "taxExempt": true,
            "taxExemptions": ["EU_REVERSE_CHARGE_EXEMPTION_RULE"]
        })
    );

    let remove_absent = staged_proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BTaxSettingsRemoveAbsent(
          $locationId: ID!,
          $taxExempt: Boolean,
          $remove: [TaxExemption!]
        ) {
          companyLocationTaxSettingsUpdate(
            companyLocationId: $locationId,
            taxExempt: $taxExempt,
            exemptionsToRemove: $remove
          ) {
            companyLocation {
              id
              taxSettings {
                taxRegistrationId
                taxExempt
                taxExemptions
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": staged_location_id,
            "remove": ["US_CA_RESELLER_EXEMPTION"]
        }),
    ));
    assert_eq!(remove_absent.status, 200);
    assert_eq!(
        remove_absent.body["data"]["companyLocationTaxSettingsUpdate"],
        json!({
            "companyLocation": {
                "id": staged_location_id,
                "taxSettings": {
                    "taxRegistrationId": "REG-1",
                    "taxExempt": true,
                    "taxExemptions": ["EU_REVERSE_CHARGE_EXEMPTION_RULE"]
                }
            },
            "userErrors": []
        })
    );

    let read_after_write = staged_proxy.process_request(json_graphql_request(
        r#"
        query RustB2BTaxSettingsReadAfterWrite($locationId: ID!) {
          companyLocation(id: $locationId) {
            id
            taxSettings {
              taxRegistrationId
              taxExempt
              taxExemptions
            }
          }
        }
        "#,
        json!({ "locationId": staged_location_id }),
    ));
    assert_eq!(read_after_write.status, 200);
    assert_eq!(
        read_after_write.body["data"]["companyLocation"],
        json!({
            "id": staged_location_id,
            "taxSettings": {
                "taxRegistrationId": "REG-1",
                "taxExempt": true,
                "taxExemptions": ["EU_REVERSE_CHARGE_EXEMPTION_RULE"]
            }
        })
    );
}

#[test]
fn b2b_location_buyer_experience_configuration_update_tail_helpers_cover_current_behavior() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "Buyer Experience Co");
    let location_id = create_b2b_location(&mut proxy, &company_id, "Buyer Experience Branch");

    let empty = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BLocationBuyerExperienceConfigurationUpdate($locationId: ID!) {
          companyLocationUpdate(companyLocationId: $locationId, input: { buyerExperienceConfiguration: {} }) {
            companyLocation { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(empty.status, 200);
    assert_eq!(
        empty.body["data"]["companyLocationUpdate"],
        json!({
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": ["input", "buyerExperienceConfiguration"],
                "message": "Invalid input.",
                "code": "INVALID_INPUT"
            }]
        })
    );

    let deposit_without_terms = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BLocationBuyerExperienceConfigurationUpdate($locationId: ID!) {
          companyLocationUpdate(companyLocationId: $locationId, input: { buyerExperienceConfiguration: { deposit: { percentage: 50.0 } } }) {
            companyLocation { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(deposit_without_terms.status, 200);
    assert_eq!(
        deposit_without_terms.body["data"]["companyLocationUpdate"],
        json!({
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": ["input", "buyerExperienceConfiguration", "deposit"],
                "message": "Deposit requires a payment terms template.",
                "code": "INVALID"
            }]
        })
    );

    let deposit_disabled = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BLocationBuyerExperienceConfigurationDepositDisabled($locationId: ID!) {
          companyLocationUpdate(companyLocationId: $locationId, input: { buyerExperienceConfiguration: { paymentTermsTemplateId: "gid://shopify/PaymentTermsTemplate/4", deposit: { percentage: 50.0 } } }) {
            companyLocation { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(deposit_disabled.status, 200);
    assert_eq!(
        deposit_disabled.body["data"]["companyLocationUpdate"],
        json!({
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": ["input", "buyerExperienceConfiguration", "deposit"],
                "message": "Deposits are not enabled for this shop.",
                "code": "INVALID"
            }]
        })
    );

    let valid = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BLocationBuyerExperienceConfigurationUpdate($locationId: ID!) {
          companyLocationUpdate(companyLocationId: $locationId, input: { buyerExperienceConfiguration: { paymentTermsTemplateId: "gid://shopify/PaymentTermsTemplate/4", checkoutToDraft: true, editableShippingAddress: true, deposit: { percentage: 50.0 } } }) {
            companyLocation {
              id
              taxSettings { taxExempt }
              buyerExperienceConfiguration {
                editableShippingAddress
                checkoutToDraft
                paymentTermsTemplate { id }
                deposit { __typename }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(valid.status, 200);
    assert_eq!(
        valid.body["data"]["companyLocationUpdate"],
        json!({
            "companyLocation": {
                "id": location_id,
                "taxSettings": { "taxExempt": false },
                "buyerExperienceConfiguration": {
                    "editableShippingAddress": true,
                    "checkoutToDraft": true,
                    "paymentTermsTemplate": { "id": "gid://shopify/PaymentTermsTemplate/4" },
                    "deposit": { "__typename": "DepositPercentage" }
                }
            },
            "userErrors": []
        })
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query RustB2BLocationBuyerExperienceConfigurationRead($locationId: ID!) {
          companyLocation(id: $locationId) {
            id
            taxSettings { taxExempt }
            buyerExperienceConfiguration {
              editableShippingAddress
              checkoutToDraft
              paymentTermsTemplate { id }
              deposit { __typename }
            }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(readback.status, 200);
    assert_eq!(
        readback.body["data"]["companyLocation"],
        valid.body["data"]["companyLocationUpdate"]["companyLocation"]
    );

    let entries = log_snapshot(&proxy)["entries"]
        .as_array()
        .expect("log entries")
        .clone();
    assert!(entries.iter().any(|entry| {
        entry["status"] == json!("failed")
            && entry["interpreted"]["primaryRootField"] == json!("companyLocationUpdate")
    }));
    assert!(entries.iter().any(|entry| {
        entry["status"] == json!("staged")
            && entry["interpreted"]["primaryRootField"] == json!("companyLocationUpdate")
    }));
}

#[test]
fn b2b_contact_location_defaults_use_shop_and_customer_context() {
    let mut proxy = snapshot_proxy();
    restore_state_with(&mut proxy, |state| {
        state["baseState"]["shop"] = json!({
            "id": "gid://shopify/Shop/singapore-b2b-context",
            "shopAddress": {
                "countryCodeV2": "SG",
                "countryCode": "SG"
            }
        });
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

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContextualDefaultsCompanyCreate {
          companyCreate(input: {
            company: { name: "Context Defaults Co" }
            companyContact: {
              firstName: "Nested"
              lastName: "Buyer"
              email: "nested-context@example.test"
            }
            companyLocation: {
              phone: "12345678"
              shippingAddress: {
                address1: "1 Context Way"
                city: "Singapore"
                countryCode: "SG"
              }
              buyerExperienceConfiguration: {
                paymentTermsTemplateId: "gid://shopify/PaymentTermsTemplate/4"
                checkoutToDraft: true
                editableShippingAddress: true
                deposit: { percentage: 25.5 }
              }
            }
          }) {
            company {
              id
              mainContact { id title locale customer { email } }
              locations(first: 1) {
                nodes {
                  id
                  locale
                  phone
                  buyerExperienceConfiguration {
                    deposit {
                      __typename
                      ... on DepositPercentage { percentage }
                    }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    let company = &create.body["data"]["companyCreate"]["company"];
    let company_id = company["id"].clone();
    assert_eq!(company["mainContact"]["title"], Value::Null);
    assert_eq!(company["mainContact"]["locale"], json!("fr"));
    assert_eq!(company["locations"]["nodes"][0]["locale"], json!("fr"));
    assert_eq!(
        company["locations"]["nodes"][0]["phone"],
        json!("+6512345678")
    );
    assert_eq!(
        company["locations"]["nodes"][0]["buyerExperienceConfiguration"]["deposit"],
        json!({
            "__typename": "DepositPercentage",
            "percentage": 25.5
        })
    );

    let explicit_nested_title = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BExplicitNestedContactTitle {
          companyCreate(input: {
            company: { name: "Explicit Title Co" }
            companyContact: {
              title: "Procurement lead"
              email: "explicit-title@example.test"
            }
          }) {
            company { mainContact { title locale } }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(
        explicit_nested_title.body["data"]["companyCreate"]["company"]["mainContact"],
        json!({
            "title": "Procurement lead",
            "locale": "fr"
        })
    );

    let later_contact = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLaterContactUsesShopContext($companyId: ID!) {
          companyContactCreate(
            companyId: $companyId
            input: {
              email: "later-context@example.test"
              phone: "87654321"
            }
          ) {
            companyContact {
              title
              locale
              customer { email phone }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id }),
    ));
    assert_eq!(
        later_contact.body["data"]["companyContactCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        later_contact.body["data"]["companyContactCreate"]["companyContact"],
        json!({
            "title": Value::Null,
            "locale": "fr",
            "customer": {
                "email": "later-context@example.test",
                "phone": "+6587654321"
            }
        })
    );

    let customer = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignContactCustomerLocale($input: CustomerInput!) {
          customerCreate(input: $input) {
            customer { id locale }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "email": "assign-context@example.test",
                "firstName": "Assigned",
                "locale": "de"
            }
        }),
    ));
    let customer_id = customer.body["data"]["customerCreate"]["customer"]["id"].clone();
    let assigned_contact = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignCustomerUsesCustomerLocale($companyId: ID!, $customerId: ID!) {
          companyAssignCustomerAsContact(companyId: $companyId, customerId: $customerId) {
            companyContact { title locale customer { id locale } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id, "customerId": customer_id }),
    ));
    assert_eq!(
        assigned_contact.body["data"]["companyAssignCustomerAsContact"]["userErrors"],
        json!([])
    );
    assert_eq!(
        assigned_contact.body["data"]["companyAssignCustomerAsContact"]["companyContact"],
        json!({
            "title": Value::Null,
            "locale": "de",
            "customer": {
                "id": customer_id,
                "locale": "de"
            }
        })
    );
}

#[test]
fn b2b_company_blank_names_reject_without_staging() {
    let mut proxy = snapshot_proxy();

    let blank_create = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BBlankCompanyCreate($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": " <b>  </b> " } }),
    ));
    assert_eq!(blank_create.status, 200);
    assert_eq!(
        blank_create.body["data"]["companyCreate"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["input", "company", "name"],
                "message": "Name can't be blank",
                "code": "BLANK"
            }]
        })
    );

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BBlankCompanyUpdateSetup($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id name locations(first: 1) { nodes { id name } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Original" } }),
    ));
    let company_id = create.body["data"]["companyCreate"]["company"]["id"].clone();
    let location_id =
        create.body["data"]["companyCreate"]["company"]["locations"]["nodes"][0]["id"].clone();

    let blank_update = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BBlankCompanyUpdate($id: ID!, $input: CompanyInput!) {
          companyUpdate(companyId: $id, input: $input) {
            company { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": company_id, "input": { "name": "   " } }),
    ));
    assert_eq!(
        blank_update.body["data"]["companyUpdate"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["input", "name"],
                "message": "Name can't be blank",
                "code": "BLANK"
            }]
        })
    );

    let read_after_blank_company = proxy.process_request(json_graphql_request(
        r#"
        query B2BBlankCompanyRead($id: ID!) {
          company(id: $id) { id name }
        }
        "#,
        json!({ "id": company_id }),
    ));
    assert_eq!(
        read_after_blank_company.body["data"]["company"]["name"],
        json!("Original")
    );

    let blank_location_update = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BBlankLocationUpdate($id: ID!, $input: CompanyLocationUpdateInput!) {
          companyLocationUpdate(companyLocationId: $id, input: $input) {
            companyLocation { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id, "input": { "name": "<i> </i>" } }),
    ));
    assert_eq!(
        blank_location_update.body["data"]["companyLocationUpdate"],
        json!({
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": ["input", "name"],
                "message": "Name can't be blank",
                "code": "BLANK"
            }]
        })
    );
}

#[test]
fn b2b_company_identity_validation_tail_helpers_cover_current_behavior() {
    let mut proxy = snapshot_proxy();

    let long_name = "x".repeat(300);
    let long_name_response = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyNameValidation($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": long_name } }),
    ));
    assert_eq!(long_name_response.status, 200);
    assert_eq!(
        long_name_response.body["data"]["companyCreate"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["input", "company", "name"],
                "message": "Name is too long (maximum is 255 characters)",
                "code": "TOO_LONG"
            }]
        })
    );

    let html_name_response = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyNameValidation($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "<b>B2B Draft</b>" } }),
    ));
    assert_eq!(html_name_response.status, 200);
    assert_eq!(
        html_name_response.body["data"]["companyCreate"],
        json!({
            "company": {
                "id": "gid://shopify/Company/1?shopify-draft-proxy=synthetic",
                "name": "B2B Draft"
            },
            "userErrors": []
        })
    );

    let long_external_id = "x".repeat(65);
    let long_external = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyExternalIdCreateValidation($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id externalId }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "B2B Draft", "externalId": long_external_id } }),
    ));
    assert_eq!(long_external.status, 200);
    assert_eq!(
        long_external.body["data"]["companyCreate"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["input", "company", "externalId"],
                "message": "External Id must be 64 characters or less.",
                "code": "TOO_LONG"
            }]
        })
    );

    let invalid_external = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyExternalIdCreateValidation($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id externalId }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "B2B Draft", "externalId": "has spaces" } }),
    ));
    assert_eq!(invalid_external.status, 200);
    assert_eq!(
        invalid_external.body["data"]["companyCreate"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["input", "company", "externalId"],
                "message": r#"External Id can only contain numbers, letters, and some special characters, including !@#$%^&*(){}[]\/?<>_-~,.;:'`""#,
                "code": "INVALID"
            }]
        })
    );

    let first = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyExternalIdCreateValidation($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id externalId }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Duplicate One", "externalId": "ACME-1" } }),
    ));
    assert_eq!(first.body["data"]["companyCreate"]["userErrors"], json!([]));
    let first_id = first.body["data"]["companyCreate"]["company"]["id"].clone();

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyExternalIdCreateValidation($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id externalId }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Duplicate Two", "externalId": "ACME-1" } }),
    ));
    assert_eq!(
        duplicate.body["data"]["companyCreate"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["input", "company", "externalId"],
                "message": "External id has already been taken.",
                "code": "TAKEN"
            }]
        })
    );

    let second = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyExternalIdCreateValidation($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id externalId }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Second", "externalId": "ACME-2" } }),
    ));
    let second_id = second.body["data"]["companyCreate"]["company"]["id"].clone();

    let self_update = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyExternalIdUpdateValidation($id: ID!, $input: CompanyInput!) {
          companyUpdate(companyId: $id, input: $input) {
            company { id externalId }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": first_id, "input": { "externalId": "ACME-1" } }),
    ));
    assert_eq!(
        self_update.body["data"]["companyUpdate"]["userErrors"],
        json!([])
    );

    let duplicate_update = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyExternalIdUpdateValidation($id: ID!, $input: CompanyInput!) {
          companyUpdate(companyId: $id, input: $input) {
            company { id externalId }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": second_id, "input": { "externalId": "ACME-1" } }),
    ));
    assert_eq!(
        duplicate_update.body["data"]["companyUpdate"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["input", "externalId"],
                "message": "External id has already been taken.",
                "code": "TAKEN"
            }]
        })
    );
}

#[test]
fn b2b_company_create_system_role_notes_match_shopify_nulls() {
    let mut proxy = snapshot_proxy();

    let bare_create = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompanyCreateSystemRoleNotesBare($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company {
              contactRoles(first: 2) {
                nodes { id name note }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "company": { "name": "System Role Notes Co" } } }),
    ));
    assert_eq!(bare_create.status, 200);
    assert_eq!(
        bare_create.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    let roles = bare_create.body["data"]["companyCreate"]["company"]["contactRoles"]["nodes"]
        .as_array()
        .expect("system roles");
    assert_eq!(roles.len(), 2);
    assert_eq!(roles[0]["name"], json!("Location admin"));
    assert_eq!(roles[0]["note"], Value::Null);
    assert_eq!(roles[1]["name"], json!("Ordering only"));
    assert_eq!(roles[1]["note"], Value::Null);

    let with_contact = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompanyCreateSystemRoleNotesContact($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company {
              mainContact {
                roleAssignments(first: 1) {
                  nodes {
                    role { name note }
                  }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "company": { "name": "System Role Assignment Notes Co" },
                "companyContact": {
                    "firstName": "Role",
                    "lastName": "Buyer",
                    "email": "system-role-notes@example.com"
                }
            }
        }),
    ));
    assert_eq!(with_contact.status, 200);
    assert_eq!(
        with_contact.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    let assignment_role = &with_contact.body["data"]["companyCreate"]["company"]["mainContact"]
        ["roleAssignments"]["nodes"][0]["role"];
    assert_eq!(assignment_role["name"], json!("Ordering only"));
    assert_eq!(assignment_role["note"], Value::Null);
}

#[test]
fn b2b_company_delete_stages_cascade_and_preserves_commit_log() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompanyDeleteSetup($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company {
              id
              locations(first: 2) { nodes { id } }
              mainContact { id }
              contactRoles(first: 2) { nodes { id } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Delete OK" } }),
    ));
    let company = &create.body["data"]["companyCreate"]["company"];
    let company_id = company["id"].clone();
    let location_id = company["locations"]["nodes"][0]["id"].clone();

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompanyDelete($id: ID!) {
          companyDelete(id: $id) {
            deletedCompanyId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": company_id }),
    ));
    assert_eq!(delete.status, 200);
    assert_eq!(
        delete.body["data"]["companyDelete"],
        json!({
            "deletedCompanyId": company_id,
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BCompanyDeleteRead($companyId: ID!, $locationId: ID!) {
          company(id: $companyId) { id name }
          companyLocation(id: $locationId) { id name }
        }
        "#,
        json!({ "companyId": company_id, "locationId": location_id }),
    ));
    assert_eq!(read.body["data"]["company"], Value::Null);
    assert_eq!(read.body["data"]["companyLocation"], Value::Null);

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("log entries");
    let delete_entry = entries
        .iter()
        .find(|entry| entry["interpreted"]["primaryRootField"] == json!("companyDelete"))
        .expect("companyDelete log entry");
    assert_eq!(delete_entry["status"], json!("staged"));
    assert_eq!(delete_entry["stagedResourceIds"], json!([company_id]));
    assert!(delete_entry["rawBody"]
        .as_str()
        .is_some_and(|raw| { raw.contains("B2BCompanyDelete") && raw.contains("companyDelete") }));
}

#[test]
fn b2b_companies_delete_mixes_blocked_deleted_and_unknown_ids() {
    let mut proxy = snapshot_proxy();

    let blocked = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompaniesDeleteBlockedSetup($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Blocked" } }),
    ));
    let blocked_id = blocked.body["data"]["companyCreate"]["company"]["id"].clone();
    let blocked_location_id =
        read_b2b_company_location_ids(&mut proxy, blocked_id.as_str().expect("blocked company id"))
            [0]
        .clone();

    let ok = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompaniesDeleteOkSetup($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Delete OK" } }),
    ));
    let ok_id = ok.body["data"]["companyCreate"]["company"]["id"].clone();

    let order = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompaniesDeleteOrderBlock($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) { order { id } userErrors { field message code } }
        }
        "#,
        json!({ "order": {
            "email": "b2b-company-delete-blocker@example.test",
            "companyLocationId": blocked_location_id,
            "lineItems": [{ "title": "Blocker", "quantity": 1 }]
        }}),
    ));
    assert_eq!(order.body["data"]["orderCreate"]["userErrors"], json!([]));

    let unknown_id = "gid://shopify/Company/999999";
    let bulk = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompaniesDelete($ids: [ID!]!) {
          companiesDelete(companyIds: $ids) {
            deletedCompanyIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": [blocked_id, ok_id, unknown_id] }),
    ));
    assert_eq!(bulk.status, 200);
    assert_eq!(
        bulk.body["data"]["companiesDelete"],
        json!({
            "deletedCompanyIds": [ok_id],
            "userErrors": [
                {
                    "field": ["companyIds", "0"],
                    "message": "Failed to delete the company.",
                    "code": "FAILED_TO_DELETE"
                },
                {
                    "field": ["companyIds", "2"],
                    "message": "Resource requested does not exist.",
                    "code": "RESOURCE_NOT_FOUND"
                }
            ]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BCompaniesDeleteRead($blocked: ID!, $deleted: ID!) {
          blocked: company(id: $blocked) { id name }
          deleted: company(id: $deleted) { id name }
        }
        "#,
        json!({ "blocked": blocked_id, "deleted": ok_id }),
    ));
    assert_eq!(
        read.body["data"]["blocked"]["id"],
        blocked.body["data"]["companyCreate"]["company"]["id"]
    );
    assert_eq!(read.body["data"]["deleted"], Value::Null);

    let entries = log_snapshot(&proxy)["entries"]
        .as_array()
        .expect("log entries")
        .clone();
    let bulk_entry = entries
        .iter()
        .find(|entry| entry["interpreted"]["primaryRootField"] == json!("companiesDelete"))
        .expect("companiesDelete log entry");
    assert_eq!(bulk_entry["status"], json!("staged"));
    assert_eq!(bulk_entry["stagedResourceIds"], json!([ok_id]));
}

#[test]
fn b2b_company_update_immutable_and_note_validation_tail_helpers_cover_current_behavior() {
    let mut proxy = snapshot_proxy();
    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyCustomerSinceCreate($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id name customerSince }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Original", "customerSince": "2024-01-01T00:00:00Z" } }),
    ));
    assert_eq!(create.status, 200);
    let company_id = create.body["data"]["companyCreate"]["company"]["id"].clone();

    let customer_since_only = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyCustomerSinceUpdate($id: ID!, $input: CompanyInput!) {
          companyUpdate(companyId: $id, input: $input) {
            company { id name customerSince }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": company_id, "input": { "customerSince": "2020-01-01T00:00:00Z" } }),
    ));
    assert_eq!(
        customer_since_only.body["data"]["companyUpdate"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["input", "customerSince"],
                "message": "This field may only be set on creation.",
                "code": "INVALID_INPUT"
            }]
        })
    );

    let read_after_reject = proxy.process_request(json_graphql_request(
        r#"
        query RustB2BCompanyCustomerSinceRead($id: ID!) {
          company(id: $id) { name customerSince }
        }
        "#,
        json!({ "id": create.body["data"]["companyCreate"]["company"]["id"].clone() }),
    ));
    assert_eq!(
        read_after_reject.body["data"]["company"],
        json!({ "name": "Original", "customerSince": "2024-01-01T00:00:00Z" })
    );

    let mixed_reject = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyCustomerSinceUpdate($id: ID!, $input: CompanyInput!) {
          companyUpdate(companyId: $id, input: $input) {
            company { id name customerSince }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": create.body["data"]["companyCreate"]["company"]["id"].clone(), "input": { "name": "Mixed Changed", "customerSince": "2020-02-01T00:00:00Z" } }),
    ));
    assert_eq!(
        mixed_reject.body["data"]["companyUpdate"]["company"],
        Value::Null
    );
    assert_eq!(
        mixed_reject.body["data"]["companyUpdate"]["userErrors"][0]["code"],
        json!("INVALID_INPUT")
    );

    let read_after_mixed_reject = proxy.process_request(json_graphql_request(
        r#"
        query RustB2BCompanyCustomerSinceRead($id: ID!) {
          company(id: $id) { name customerSince }
        }
        "#,
        json!({ "id": create.body["data"]["companyCreate"]["company"]["id"].clone() }),
    ));
    assert_eq!(
        read_after_mixed_reject.body["data"]["company"],
        json!({ "name": "Original", "customerSince": "2024-01-01T00:00:00Z" })
    );

    let null_reject = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyCustomerSinceUpdate($id: ID!, $input: CompanyInput!) {
          companyUpdate(companyId: $id, input: $input) {
            company { id name customerSince }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": create.body["data"]["companyCreate"]["company"]["id"].clone(), "input": { "customerSince": Value::Null } }),
    ));
    assert_eq!(
        null_reject.body["data"]["companyUpdate"]["company"],
        Value::Null
    );
    assert_eq!(
        null_reject.body["data"]["companyUpdate"]["userErrors"][0]["field"],
        json!(["input", "customerSince"])
    );

    let html_note = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyNoteValidation($id: ID!, $input: CompanyInput!) {
          companyUpdate(companyId: $id, input: $input) {
            company { id note }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": create.body["data"]["companyCreate"]["company"]["id"].clone(), "input": { "note": "<b>merchant update note</b>" } }),
    ));
    assert_eq!(
        html_note.body["data"]["companyUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        html_note.body["data"]["companyUpdate"]["company"]["note"],
        json!("<b>merchant update note</b>")
    );

    let read_after_note = proxy.process_request(json_graphql_request(
        r#"
        query RustB2BCompanyNoteRead($id: ID!) {
          company(id: $id) { note }
        }
        "#,
        json!({ "id": create.body["data"]["companyCreate"]["company"]["id"].clone() }),
    ));
    assert_eq!(
        read_after_note.body["data"]["company"]["note"],
        json!("<b>merchant update note</b>")
    );

    let invalid_note = format!("<script>{}</script>", "x".repeat(6000));
    let note_reject = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyNoteValidation($id: ID!, $input: CompanyInput!) {
          companyUpdate(companyId: $id, input: $input) {
            company { id note }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": create.body["data"]["companyCreate"]["company"]["id"].clone(), "input": { "note": invalid_note } }),
    ));
    assert_eq!(
        note_reject.body["data"]["companyUpdate"]["company"],
        Value::Null
    );
    assert_eq!(
        note_reject.body["data"]["companyUpdate"]["userErrors"],
        json!([{
            "field": ["input", "notes"],
            "message": "Notes is too long (maximum is 5000 characters)",
            "code": "TOO_LONG"
        }])
    );
}

#[test]
fn b2b_unknown_update_ids_return_resource_not_found_without_staging() {
    let mut proxy = snapshot_proxy();
    let unknown_company_id = "gid://shopify/Company/999";
    let unknown_location_id = "gid://shopify/CompanyLocation/999";
    let unknown_tax_location_id = "gid://shopify/CompanyLocation/998";

    let company_update = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyUnknownResource($companyId: ID!) {
          companyUpdate(companyId: $companyId, input: { name: "X" }) {
            company { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": unknown_company_id }),
    ));
    assert_eq!(company_update.status, 200);
    assert_eq!(
        company_update.body["data"]["companyUpdate"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["companyId"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let location_update = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BLocationBuyerExperienceConfigurationUnknownResource($companyLocationId: ID!) {
          companyLocationUpdate(
            companyLocationId: $companyLocationId,
            input: {
              buyerExperienceConfiguration: {
                paymentTermsTemplateId: "gid://shopify/PaymentTermsTemplate/4",
                checkoutToDraft: true,
                editableShippingAddress: true
              }
            }
          ) {
            companyLocation { id buyerExperienceConfiguration { checkoutToDraft } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyLocationId": unknown_location_id }),
    ));
    assert_eq!(location_update.status, 200);
    assert_eq!(
        location_update.body["data"]["companyLocationUpdate"],
        json!({
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": ["input"],
                "message": "The company location doesn't exist",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let tax_update = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BTaxSettingsUnknownResource($companyLocationId: ID!) {
          companyLocationTaxSettingsUpdate(companyLocationId: $companyLocationId, taxExempt: true) {
            companyLocation { id name taxSettings { taxExempt } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyLocationId": unknown_tax_location_id }),
    ));
    assert_eq!(tax_update.status, 200);
    assert_eq!(
        tax_update.body["data"]["companyLocationTaxSettingsUpdate"],
        json!({
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": ["companyLocationId"],
                "message": "The company location doesn't exist",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let company_read = proxy.process_request(json_graphql_request(
        r#"
        query RustB2BCompanyUnknownResourceRead($companyId: ID!) {
          company(id: $companyId) { id name }
        }
        "#,
        json!({ "companyId": unknown_company_id }),
    ));
    assert_eq!(company_read.body["data"]["company"], Value::Null);

    let location_read = proxy.process_request(json_graphql_request(
        r#"
        query RustB2BLocationBuyerExperienceConfigurationUnknownResourceRead($companyLocationId: ID!) {
          companyLocation(id: $companyLocationId) { id name buyerExperienceConfiguration { checkoutToDraft } }
        }
        "#,
        json!({ "companyLocationId": unknown_location_id }),
    ));
    assert_eq!(location_read.body["data"]["companyLocation"], Value::Null);

    let tax_location_read = proxy.process_request(json_graphql_request(
        r#"
        query RustB2BLocationBuyerExperienceConfigurationUnknownResourceRead($companyLocationId: ID!) {
          companyLocation(id: $companyLocationId) { id name taxSettings { taxExempt } }
        }
        "#,
        json!({ "companyLocationId": unknown_tax_location_id }),
    ));
    assert_eq!(
        tax_location_read.body["data"]["companyLocation"],
        Value::Null
    );

    let entries = log_snapshot(&proxy)["entries"]
        .as_array()
        .expect("log entries")
        .clone();
    for root in [
        "companyUpdate",
        "companyLocationUpdate",
        "companyLocationTaxSettingsUpdate",
    ] {
        let entry = entries
            .iter()
            .find(|entry| entry["interpreted"]["primaryRootField"] == json!(root))
            .unwrap_or_else(|| panic!("missing {root} log entry"));
        assert_eq!(entry["status"], json!("failed"));
        assert_eq!(entry["stagedResourceIds"], json!([]));
    }
}

#[test]
fn b2b_magic_synthetic_company_location_id_is_not_treated_as_existing() {
    let mut proxy = snapshot_proxy();
    let location_id = "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic";

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BMagicLocationUpdate($id: ID!) {
          companyLocationUpdate(
            companyLocationId: $id,
            input: {
              buyerExperienceConfiguration: {
                paymentTermsTemplateId: "gid://shopify/PaymentTermsTemplate/4",
                checkoutToDraft: true,
                editableShippingAddress: true
              }
            }
          ) {
            companyLocation { id name taxSettings { taxExempt } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(
        update.body["data"]["companyLocationUpdate"],
        json!({
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": ["input"],
                "message": "The company location doesn't exist",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let tax = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BMagicLocationTax($id: ID!) {
          companyLocationTaxSettingsUpdate(companyLocationId: $id, taxExempt: true) {
            companyLocation { id name billingAddress { address1 } taxSettings { taxExempt } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(
        tax.body["data"]["companyLocationTaxSettingsUpdate"],
        json!({
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": ["companyLocationId"],
                "message": "The company location doesn't exist",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let credit = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BMagicLocationStoreCredit($id: ID!) {
          storeCreditAccountCredit(id: $id, creditInput: { creditAmount: { amount: "1.00", currencyCode: USD } }) {
            storeCreditAccountTransaction { account { owner { ... on CompanyLocation { id name } } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(
        credit.body["data"]["storeCreditAccountCredit"]["storeCreditAccountTransaction"],
        Value::Null
    );
    assert_eq!(
        credit.body["data"]["storeCreditAccountCredit"]["userErrors"],
        json!([{
            "field": ["id"],
            "message": "Owner does not exist",
            "code": "OWNER_NOT_FOUND"
        }])
    );
}

#[test]
fn b2b_companies_cold_live_hybrid_read_forwards_upstream() {
    let upstream_body = json!({
        "data": {
            "companies": {
                "nodes": [{
                    "id": "gid://shopify/Company/upstream",
                    "name": "Upstream Company"
                }]
            }
        }
    });
    let captured = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        let upstream_body = upstream_body.clone();
        move |request| {
            captured.lock().expect("captured upstream").push(request);
            Response {
                status: 200,
                headers: Default::default(),
                body: upstream_body.clone(),
            }
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        query B2BColdCompanies {
          companies(first: 1) { nodes { id name } }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(response.body, upstream_body);
    let calls = captured.lock().expect("captured upstream");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].body.contains("companies(first: 1)"));
}

#[test]
fn b2b_company_location_cold_live_hybrid_read_hydrates_for_local_update() {
    let location_id = "gid://shopify/CompanyLocation/880001";
    let company_id = "gid://shopify/Company/780001";
    let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).expect("upstream body");
            assert!(
                !body["query"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("mutation"),
                "supported B2B writes must not be forwarded upstream"
            );
            let mutation_hydrate =
                body["operationName"].as_str() == Some("B2BMutationTargetsHydrate");
            captured.lock().expect("captured upstream").push(body);
            if mutation_hydrate {
                b2b_mutation_targets_location_hydrate_response(
                    location_id,
                    "Cold Upstream HQ",
                    company_id,
                )
            } else {
                b2b_company_location_hydrate_response(location_id, "Cold Upstream HQ", company_id)
            }
        }
    });

    let cold_read = proxy.process_request(json_graphql_request(
        r#"
        query B2BColdCompanyLocation($id: ID!) {
          companyLocation(id: $id) { id name company { id name } }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(
        cold_read.body["data"]["companyLocation"],
        json!({
            "id": location_id,
            "name": "Cold Upstream HQ",
            "company": { "id": company_id, "name": "Hydrated Company" }
        })
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BUpdateColdCompanyLocation($id: ID!) {
          companyLocationUpdate(companyLocationId: $id, input: { name: "Locally Updated HQ" }) {
            companyLocation { id name company { id name } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(
        update.body["data"]["companyLocationUpdate"],
        json!({
            "companyLocation": {
                "id": location_id,
                "name": "Locally Updated HQ",
                "company": { "id": company_id, "name": "Hydrated Company" }
            },
            "userErrors": []
        })
    );
    let calls = captured.lock().expect("captured upstream");
    assert_eq!(calls.len(), 2);
    assert!(calls[0]["query"]
        .as_str()
        .is_some_and(|query| query.contains("query B2BColdCompanyLocation")));
    assert_eq!(
        calls[1]["operationName"],
        json!("B2BMutationTargetsHydrate")
    );
}

#[test]
fn b2b_company_contact_cold_live_hybrid_read_forwards_authoritative_relationships() {
    let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).expect("upstream body");
            captured.lock().expect("captured upstream").push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "companyContact": b2b_mutation_first_hydrate_test_contact()
                    }
                }),
            }
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        query B2BColdCompanyContact($id: ID!) {
          companyContact(id: $id) {
            id
            title
            company { id name }
            customer { id email }
            roleAssignments(first: 5) {
              nodes { id companyLocation { id } role { id } }
            }
          }
        }
        "#,
        json!({ "id": B2B_HYDRATE_TEST_CONTACT_ID }),
    ));
    assert_eq!(
        response.body["data"]["companyContact"]["company"],
        json!({
            "id": B2B_HYDRATE_TEST_COMPANY_ID,
            "name": "Hydrated Buyer"
        })
    );
    assert_eq!(
        response.body["data"]["companyContact"]["customer"],
        json!({
            "id": B2B_HYDRATE_TEST_CUSTOMER_ID,
            "email": "buyer@example.com"
        })
    );
    assert_eq!(
        response.body["data"]["companyContact"]["roleAssignments"]["nodes"][0]["id"],
        json!(B2B_HYDRATE_TEST_ROLE_ASSIGNMENT_ID)
    );
    assert_eq!(captured.lock().expect("captured upstream").len(), 1);
}

#[test]
fn b2b_company_update_hydrates_an_unseeded_target_and_keeps_unrelated_reads_visible() {
    let company_id = "gid://shopify/Company/780010";
    let unrelated_company_id = "gid://shopify/Company/780011";
    let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).expect("upstream body");
            let query = body["query"].as_str().unwrap_or_default();
            assert!(
                !query.contains("mutation"),
                "supported B2B writes must only issue query hydration"
            );
            captured
                .lock()
                .expect("captured upstream")
                .push(body.clone());
            if body["variables"]["id"] == json!(unrelated_company_id)
                || body["variables"]["ids"]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|id| id == unrelated_company_id))
            {
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "company": {
                                "id": unrelated_company_id,
                                "name": "Unrelated Upstream Company"
                            }
                        }
                    }),
                }
            } else {
                Response {
                    status: 200,
                    headers: Default::default(),
                    body: json!({
                        "data": {
                            "companies": {
                                "nodes": [{
                                    "id": company_id,
                                    "name": "Mutation First Company",
                                    "externalId": "MUTATION-FIRST",
                                    "locations": { "nodes": [] },
                                    "contacts": { "nodes": [] },
                                    "contactRoles": { "nodes": [] }
                                }]
                            },
                            "nodes": [{
                                "__typename": "Company",
                                "id": company_id,
                                "name": "Mutation First Company",
                                "externalId": "MUTATION-FIRST",
                                "locations": { "nodes": [] },
                                "contacts": { "nodes": [] },
                                "contactRoles": { "nodes": [] }
                            }]
                        }
                    }),
                }
            }
        }
    });

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BMutationFirstCompanyUpdate($companyId: ID!) {
          companyUpdate(companyId: $companyId, input: { name: "Locally Updated Company" }) {
            company { id name externalId }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id }),
    ));
    assert_eq!(
        update.body["data"]["companyUpdate"],
        json!({
            "company": {
                "id": company_id,
                "name": "Locally Updated Company",
                "externalId": "MUTATION-FIRST"
            },
            "userErrors": []
        })
    );

    let unrelated = proxy.process_request(json_graphql_request(
        r#"
        query B2BUnrelatedCompanyAfterMutationFirst($id: ID!) {
          company(id: $id) { id name }
        }
        "#,
        json!({ "id": unrelated_company_id }),
    ));
    assert_eq!(
        unrelated.body["data"]["company"],
        json!({
            "id": unrelated_company_id,
            "name": "Unrelated Upstream Company"
        })
    );
    assert_eq!(captured.lock().expect("captured upstream").len(), 2);
}

#[test]
fn b2b_mutation_first_hydrates_contact_role_staff_address_and_customer_relationships() {
    let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).expect("upstream body");
            assert!(
                !body["query"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("mutation"),
                "supported B2B mutations must hydrate through queries only"
            );
            let response = b2b_mutation_first_hydrate_test_response(&body);
            captured.lock().expect("captured upstream").push(body);
            response
        }
    });

    let contact_update = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BMutationFirstContactUpdate($id: ID!) {
          companyContactUpdate(companyContactId: $id, input: { title: "Hydrated buyer" }) {
            companyContact { id title company { id name } customer { id email } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": B2B_HYDRATE_TEST_CONTACT_ID }),
    ));
    assert_eq!(
        contact_update.body["data"]["companyContactUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        contact_update.body["data"]["companyContactUpdate"]["companyContact"]["title"],
        json!("Hydrated buyer")
    );
    let post_contact_dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    assert_eq!(
        post_contact_dump.body["state"]["baseState"]["b2bCustomers"][B2B_HYDRATE_TEST_CUSTOMER_ID]
            ["email"],
        json!("buyer@example.com")
    );
    assert!(post_contact_dump.body["state"]["stagedState"]["customers"]
        .get(B2B_HYDRATE_TEST_CUSTOMER_ID)
        .is_none());

    let delete_address = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BMutationFirstAddressDelete($id: ID!) {
          companyAddressDelete(addressId: $id) {
            deletedAddressId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": B2B_HYDRATE_TEST_ADDRESS_ID }),
    ));
    assert_eq!(
        delete_address.body["data"]["companyAddressDelete"],
        json!({
            "deletedAddressId": B2B_HYDRATE_TEST_ADDRESS_ID,
            "userErrors": []
        })
    );

    let revoke_role = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BMutationFirstRoleRevoke($contactId: ID!, $assignmentId: ID!) {
          companyContactRevokeRole(
            companyContactId: $contactId
            companyContactRoleAssignmentId: $assignmentId
          ) {
            revokedCompanyContactRoleAssignmentId
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "contactId": B2B_HYDRATE_TEST_CONTACT_ID,
            "assignmentId": B2B_HYDRATE_TEST_ROLE_ASSIGNMENT_ID
        }),
    ));
    assert_eq!(
        revoke_role.body["data"]["companyContactRevokeRole"],
        json!({
            "revokedCompanyContactRoleAssignmentId": B2B_HYDRATE_TEST_ROLE_ASSIGNMENT_ID,
            "userErrors": []
        })
    );

    let remove_staff = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BMutationFirstStaffRemove($ids: [ID!]!) {
          companyLocationRemoveStaffMembers(companyLocationStaffMemberAssignmentIds: $ids) {
            deletedCompanyLocationStaffMemberAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": [B2B_HYDRATE_TEST_STAFF_ASSIGNMENT_ID] }),
    ));
    assert_eq!(
        remove_staff.body["data"]["companyLocationRemoveStaffMembers"],
        json!({
            "deletedCompanyLocationStaffMemberAssignmentIds": [B2B_HYDRATE_TEST_STAFF_ASSIGNMENT_ID],
            "userErrors": []
        })
    );

    let assign_staff = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BMutationFirstStaffAssign($locationId: ID!, $staffIds: [ID!]!) {
          companyLocationAssignStaffMembers(
            companyLocationId: $locationId
            staffMemberIds: $staffIds
          ) {
            companyLocationStaffMemberAssignments { staffMember { id } companyLocation { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": B2B_HYDRATE_TEST_LOCATION_ID,
            "staffIds": [B2B_HYDRATE_TEST_STAFF_ID]
        }),
    ));
    assert_eq!(
        assign_staff.body["data"]["companyLocationAssignStaffMembers"]["userErrors"],
        json!([])
    );
    assert_eq!(
        assign_staff.body["data"]["companyLocationAssignStaffMembers"]
            ["companyLocationStaffMemberAssignments"][0]["staffMember"]["id"],
        json!(B2B_HYDRATE_TEST_STAFF_ID)
    );

    let assign_customer = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BMutationFirstAssignCustomer($companyId: ID!, $customerId: ID!) {
          companyAssignCustomerAsContact(companyId: $companyId, customerId: $customerId) {
            companyContact { id company { id } customer { id email } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyId": B2B_HYDRATE_TEST_COMPANY_ID,
            "customerId": B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID
        }),
    ));
    assert_eq!(
        assign_customer.body["data"]["companyAssignCustomerAsContact"]["userErrors"],
        json!([])
    );
    assert_eq!(
        assign_customer.body["data"]["companyAssignCustomerAsContact"]["companyContact"]
            ["customer"]["id"],
        json!(B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID)
    );

    let calls = captured.lock().expect("captured upstream");
    assert!(calls.iter().all(|body| body["query"]
        .as_str()
        .is_some_and(|query| !query.contains("mutation"))));
    assert!(calls.iter().all(|body| {
        !matches!(
            body["operationName"].as_str(),
            Some(
                "B2BCompanyAddressOwnerHydrate"
                    | "B2BCompanyCatalogPageHydrate"
                    | "B2BLocationAssignmentsPageHydrate"
            )
        )
    }));
    assert_eq!(
        calls
            .iter()
            .filter(|body| { body["operationName"] == json!("B2BMutationTargetsHydrate") })
            .count(),
        5,
        "already staged relationship targets must not be hydrated again"
    );
}

#[test]
fn b2b_assign_customer_as_contact_mutation_first_hydrates_and_reads_relationship() {
    let captured = Arc::new(Mutex::new(Vec::<Request>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            assert_eq!(request.method, "POST");
            assert_eq!(request.path, "/admin/api/2026-04/graphql.json");
            assert_eq!(
                request.headers.get("X-Shopify-Access-Token"),
                Some(&"assign-customer-token".to_string())
            );
            let body = serde_json::from_str::<Value>(&request.body).expect("upstream body");
            assert_eq!(body["operationName"], json!("B2BMutationTargetsHydrate"));
            assert!(body["query"]
                .as_str()
                .is_some_and(|query| !query.contains("mutation")));
            captured
                .lock()
                .expect("captured upstream")
                .push(request.clone());
            b2b_mutation_first_hydrate_test_response(&body)
        }
    });

    let mutation = r#"
        mutation B2BAssignCustomerAsContactMutationFirst($companyId: ID!, $customerId: ID!) {
          companyAssignCustomerAsContact(companyId: $companyId, customerId: $customerId) {
            companyContact {
              id
              isMainContact
              company { id name }
              customer { id email firstName lastName }
            }
            userErrors { field message code }
          }
        }
    "#;
    let mut request = json_graphql_request(
        mutation,
        json!({
            "companyId": B2B_HYDRATE_TEST_COMPANY_ID,
            "customerId": B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID
        }),
    );
    request.headers.insert(
        "X-Shopify-Access-Token".to_string(),
        "assign-customer-token".to_string(),
    );
    let assigned = proxy.process_request(request);
    assert_eq!(assigned.status, 200);
    assert_eq!(
        assigned.body["data"]["companyAssignCustomerAsContact"]["userErrors"],
        json!([])
    );
    let contact_id = assigned.body["data"]["companyAssignCustomerAsContact"]["companyContact"]
        ["id"]
        .as_str()
        .expect("assigned contact id")
        .to_string();

    let calls = captured.lock().expect("captured upstream");
    assert_eq!(calls.len(), 1, "submitted ids should hydrate in one batch");
    let hydrated_ids = serde_json::from_str::<Value>(&calls[0].body).expect("upstream body")
        ["variables"]["ids"]
        .as_array()
        .expect("batched hydrate ids")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();
    assert_eq!(
        hydrated_ids,
        vec![
            B2B_HYDRATE_TEST_COMPANY_ID.to_string(),
            B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID.to_string()
        ]
    );
    drop(calls);

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query B2BAssignCustomerAsContactMutationFirstReadback(
          $companyId: ID!
          $companyContactId: ID!
          $customerId: ID!
        ) {
          company(id: $companyId) {
            id
            contacts(first: 5) { nodes { id } }
          }
          companyContact(id: $companyContactId) {
            id
            company { id }
            customer { id email }
          }
          customer(id: $customerId) {
            id
            companyContactProfiles { id company { id } }
          }
        }
        "#,
        json!({
            "companyId": B2B_HYDRATE_TEST_COMPANY_ID,
            "companyContactId": contact_id,
            "customerId": B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID
        }),
    ));
    assert_eq!(readback.status, 200);
    assert_eq!(
        readback.body["errors"],
        Value::Null,
        "readback: {}",
        readback.body
    );
    assert_eq!(
        readback.body["data"]["company"]["contacts"]["nodes"][1]["id"],
        json!(contact_id)
    );
    assert_eq!(
        readback.body["data"]["companyContact"]["customer"]["id"],
        json!(B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID)
    );
    assert_eq!(
        readback.body["data"]["customer"]["companyContactProfiles"][0]["id"],
        json!(contact_id)
    );
    assert_eq!(captured.lock().expect("captured upstream").len(), 1);

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["state"]["baseState"]["b2bCustomers"][B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID]
            ["email"],
        json!("assignable@example.com"),
        "upstream customer observations belong to base state"
    );
    assert!(
        dump.body["state"]["stagedState"]["customers"]
            .get(B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID)
            .is_none(),
        "assigning a customer as a contact must not stage the observed customer itself"
    );

    let log = log_snapshot(&proxy);
    let entry = log["entries"]
        .as_array()
        .expect("log entries")
        .iter()
        .find(|entry| {
            entry["interpreted"]["primaryRootField"] == json!("companyAssignCustomerAsContact")
        })
        .expect("assign customer log entry");
    assert_eq!(entry["status"], json!("staged"));
    let raw_body = serde_json::from_str::<Value>(
        entry["rawBody"]
            .as_str()
            .expect("assign customer raw request body"),
    )
    .expect("assign customer raw request JSON");
    assert_eq!(raw_body["query"], json!(mutation));
}

#[test]
fn b2b_assign_customer_as_contact_mutation_first_returns_captured_missing_resource_errors() {
    let cases = [
        (
            "gid://shopify/Company/does-not-exist",
            B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID,
            json!({
                "companyContact": null,
                "userErrors": [{
                    "field": ["companyId"],
                    "message": "Company does not exist.",
                    "code": "RESOURCE_NOT_FOUND"
                }]
            }),
        ),
        (
            B2B_HYDRATE_TEST_COMPANY_ID,
            "gid://shopify/Customer/does-not-exist",
            json!({
                "companyContact": null,
                "userErrors": [{
                    "field": ["customerId"],
                    "message": "Customer does not exist.",
                    "code": "RESOURCE_NOT_FOUND"
                }]
            }),
        ),
        (
            "gid://shopify/Company/does-not-exist",
            "gid://shopify/Customer/does-not-exist",
            json!({
                "companyContact": null,
                "userErrors": [{
                    "field": ["companyId"],
                    "message": "Company does not exist.",
                    "code": "RESOURCE_NOT_FOUND"
                }]
            }),
        ),
    ];

    for (company_id, customer_id, expected) in cases {
        let mut proxy =
            configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport(|request| {
                let body = serde_json::from_str::<Value>(&request.body).expect("upstream body");
                b2b_mutation_first_hydrate_test_response(&body)
            });
        let response = proxy.process_request(json_graphql_request(
            r#"
            mutation B2BAssignCustomerAsContactMissingResources($companyId: ID!, $customerId: ID!) {
              companyAssignCustomerAsContact(companyId: $companyId, customerId: $customerId) {
                companyContact { id }
                userErrors { field message code }
              }
            }
            "#,
            json!({ "companyId": company_id, "customerId": customer_id }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["companyAssignCustomerAsContact"], expected,
            "unexpected missing-resource result for company={company_id} customer={customer_id}"
        );
    }
}

#[test]
fn b2b_mutation_hydration_pages_partial_company_memberships_before_delete_decisions() {
    let company_id = "gid://shopify/Company/991100";
    let deleted_location_id = "gid://shopify/CompanyLocation/992100";
    let sibling_location_id = "gid://shopify/CompanyLocation/992101";
    let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).expect("upstream body");
            assert!(
                !body["query"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("mutation"),
                "membership hydration must stay query-only"
            );
            let operation_name = body["operationName"].as_str().unwrap_or_default();
            let response = match operation_name {
                "B2BMutationTargetsHydrate" => json!({
                    "data": {
                        "nodes": [{
                            "__typename": "CompanyLocation",
                            "id": deleted_location_id,
                            "name": "Paged primary",
                            "roleAssignments": {
                                "nodes": [],
                                "pageInfo": { "hasNextPage": false, "endCursor": Value::Null }
                            },
                            "staffMemberAssignments": {
                                "nodes": [],
                                "pageInfo": { "hasNextPage": false, "endCursor": Value::Null }
                            },
                            "company": {
                                "__typename": "Company",
                                "id": company_id,
                                "name": "Paged company",
                                "locations": {
                                    "nodes": [{ "id": deleted_location_id, "name": "Paged primary" }],
                                    "pageInfo": { "hasNextPage": true, "endCursor": "page-one" }
                                },
                                "contacts": {
                                    "nodes": [],
                                    "pageInfo": { "hasNextPage": false, "endCursor": Value::Null }
                                },
                                "contactRoles": {
                                    "nodes": [],
                                    "pageInfo": { "hasNextPage": false, "endCursor": Value::Null }
                                },
                                "mainContact": Value::Null
                            }
                        }]
                    }
                }),
                _ => json!({
                    "data": {
                        "companyLocation": {
                            "id": sibling_location_id,
                            "name": "Paged sibling",
                            "company": { "id": company_id, "name": "Paged company" }
                        }
                    }
                }),
            };
            captured.lock().expect("captured upstream").push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: response,
            }
        }
    });

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BDeleteAfterMembershipPages($id: ID!) {
          companyLocationDelete(companyLocationId: $id) {
            deletedCompanyLocationId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": deleted_location_id }),
    ));
    assert_eq!(
        delete.body["data"]["companyLocationDelete"],
        json!({ "deletedCompanyLocationId": deleted_location_id, "userErrors": [] })
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    assert_eq!(dump.status, 200);
    assert_eq!(
        dump.body["state"]["baseState"]["b2bRelationshipCompleteness"]
            [format!("company:{company_id}:locations")],
        json!("partial"),
        "a page with hasNextPage must remain explicit partial evidence"
    );

    let deleted_read = proxy.process_request(json_graphql_request(
        "query B2BDeletedPagedLocation($id: ID!) { companyLocation(id: $id) { id } }",
        json!({ "id": deleted_location_id }),
    ));
    assert!(deleted_read.body["data"]["companyLocation"].is_null());

    let sibling_read = proxy.process_request(json_graphql_request(
        "query B2BSiblingPagedLocation($id: ID!) { companyLocation(id: $id) { id name company { id name } } }",
        json!({ "id": sibling_location_id }),
    ));
    assert_eq!(
        sibling_read.body["data"]["companyLocation"],
        json!({
            "id": sibling_location_id,
            "name": "Paged sibling",
            "company": { "id": company_id, "name": "Paged company" }
        })
    );

    let calls = captured.lock().expect("captured upstream");
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0]["operationName"],
        json!("B2BMutationTargetsHydrate")
    );
    assert!(calls[1]["query"]
        .as_str()
        .is_some_and(|query| query.contains("query B2BSiblingPagedLocation")));
    assert!(calls
        .iter()
        .all(|call| { call["operationName"] != json!("B2BCompanyCatalogPageHydrate") }));
}

#[test]
fn b2b_mutation_hydration_batches_and_deduplicates_many_ids_without_child_page_walks() {
    let unique_company_ids = (0..25)
        .map(|index| format!("gid://shopify/Company/{}", 9_810_000 + index))
        .collect::<Vec<_>>();
    let submitted_company_ids = unique_company_ids
        .iter()
        .chain(unique_company_ids.iter())
        .cloned()
        .collect::<Vec<_>>();
    let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).expect("upstream body");
            assert_eq!(
                body["operationName"],
                json!("B2BMutationTargetsHydrate"),
                "multi-page child relationships must not trigger follow-up catalog requests"
            );
            let nodes = body["variables"]["ids"]
                .as_array()
                .expect("batched ids")
                .iter()
                .filter_map(Value::as_str)
                .enumerate()
                .map(|(index, company_id)| {
                    let location_id = format!("gid://shopify/CompanyLocation/{}", 9_820_000 + index);
                    let contact_id = format!("gid://shopify/CompanyContact/{}", 9_830_000 + index);
                    json!({
                        "__typename": "Company",
                        "id": company_id,
                        "name": format!("Large company {index}"),
                        "locations": {
                            "nodes": [{
                                "__typename": "CompanyLocation",
                                "id": location_id,
                                "name": format!("Location {index}"),
                                "roleAssignments": {
                                    "nodes": [],
                                    "pageInfo": { "hasNextPage": true, "endCursor": "location-role-page" }
                                },
                                "staffMemberAssignments": {
                                    "nodes": [],
                                    "pageInfo": { "hasNextPage": true, "endCursor": "staff-page" }
                                }
                            }],
                            "pageInfo": { "hasNextPage": true, "endCursor": "location-page" }
                        },
                        "contacts": {
                            "nodes": [{
                                "__typename": "CompanyContact",
                                "id": contact_id,
                                "title": "Buyer",
                                "roleAssignments": {
                                    "nodes": [],
                                    "pageInfo": { "hasNextPage": true, "endCursor": "contact-role-page" }
                                }
                            }],
                            "pageInfo": { "hasNextPage": true, "endCursor": "contact-page" }
                        },
                        "contactRoles": {
                            "nodes": [],
                            "pageInfo": { "hasNextPage": true, "endCursor": "role-page" }
                        }
                    })
                })
                .collect::<Vec<_>>();
            captured.lock().expect("captured upstream").push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": nodes } }),
            }
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BDeleteManyCompanies($companyIds: [ID!]!) {
          companiesDelete(companyIds: $companyIds) {
            deletedCompanyIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyIds": submitted_company_ids }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companiesDelete"]["deletedCompanyIds"]
            .as_array()
            .expect("deleted company ids")
            .len(),
        50,
        "hydration deduplication must not rewrite the mutation's submitted result order"
    );

    let calls = captured.lock().expect("captured upstream");
    assert_eq!(calls.len(), 1, "50 submitted IDs should use one node batch");
    let hydrated_ids = calls[0]["variables"]["ids"]
        .as_array()
        .expect("hydrated ids");
    assert_eq!(hydrated_ids.len(), 25, "duplicate IDs must be removed");
    assert!(calls[0]["query"]
        .as_str()
        .is_some_and(|query| !query.contains("locations(first: 50)")
            && !query.contains("contacts(first: 50)")
            && !query.contains("contactRoles(first: 50)")));
}

#[test]
fn b2b_cold_address_delete_uses_direct_identity_and_masks_later_owner_observation() {
    let address_id = "gid://shopify/CompanyAddress/9891001";
    let location_id = "gid://shopify/CompanyLocation/9892001";
    let company_id = "gid://shopify/Company/9893001";
    let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).expect("upstream body");
            let response_body = if body["operationName"] == json!("B2BMutationTargetsHydrate") {
                json!({
                    "data": {
                        "nodes": [{
                            "__typename": "CompanyAddress",
                            "id": address_id,
                            "address1": "1 Indexed Way",
                            "companyName": "Indexed buyer",
                            "countryCode": "CA"
                        }]
                    }
                })
            } else {
                json!({
                    "data": {
                        "companyLocation": {
                            "__typename": "CompanyLocation",
                            "id": location_id,
                            "name": "Indexed HQ",
                            "billingAddress": {
                                "__typename": "CompanyAddress",
                                "id": address_id,
                                "address1": "1 Indexed Way",
                                "companyName": "Indexed buyer",
                                "countryCode": "CA"
                            },
                            "shippingAddress": Value::Null,
                            "company": {
                                "__typename": "Company",
                                "id": company_id,
                                "name": "Indexed buyer"
                            }
                        }
                    }
                })
            };
            captured.lock().expect("captured upstream").push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: response_body,
            }
        }
    });

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BDeleteColdAddress($addressId: ID!) {
          companyAddressDelete(addressId: $addressId) {
            deletedAddressId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "addressId": address_id }),
    ));
    assert_eq!(
        delete.body["data"]["companyAddressDelete"],
        json!({ "deletedAddressId": address_id, "userErrors": [] })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BReadColdAddressOwner($locationId: ID!) {
          companyLocation(id: $locationId) {
            id
            billingAddress { id }
            shippingAddress { id }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(
        read.body["data"]["companyLocation"],
        json!({
            "id": location_id,
            "billingAddress": Value::Null,
            "shippingAddress": Value::Null
        })
    );

    let dump = proxy.process_request(request_with_body("POST", "/__meta/dump", "{}"));
    assert!(dump.body["state"]["stagedState"]["deletedB2bAddressIds"]
        .as_array()
        .is_some_and(|ids| ids.iter().any(|id| id == address_id)));
    let calls = captured.lock().expect("captured upstream");
    assert_eq!(calls.len(), 2);
    assert!(calls
        .iter()
        .all(|call| { call["operationName"] != json!("B2BCompanyAddressOwnerHydrate") }));
}

#[test]
fn b2b_role_membership_hydration_batches_targets_and_does_not_page_assignments() {
    let location_id = "gid://shopify/CompanyLocation/9882001";
    let company_id = "gid://shopify/Company/9883001";
    let contacts = (0..25)
        .map(|index| format!("gid://shopify/CompanyContact/{}", 9_884_000 + index))
        .collect::<Vec<_>>();
    let roles = (0..25)
        .map(|index| format!("gid://shopify/CompanyContactRole/{}", 9_885_000 + index))
        .collect::<Vec<_>>();
    let roles_to_assign = contacts
        .iter()
        .zip(roles.iter())
        .map(|(contact_id, role_id)| {
            json!({
                "companyContactId": contact_id,
                "companyContactRoleId": role_id
            })
        })
        .collect::<Vec<_>>();
    let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).expect("upstream body");
            assert_eq!(body["operationName"], json!("B2BMutationTargetsHydrate"));
            let nodes = body["variables"]["ids"]
                .as_array()
                .expect("batched role targets")
                .iter()
                .filter_map(Value::as_str)
                .map(|id| match id.rsplit('/').next().and_then(|tail| tail.parse::<usize>().ok()) {
                    Some(tail) if id.starts_with("gid://shopify/CompanyContact/") => {
                        let index = tail - 9_884_000;
                        let duplicate = index % 2 == 0;
                        json!({
                            "__typename": "CompanyContact",
                            "id": id,
                            "title": format!("Buyer {index}"),
                            "company": { "__typename": "Company", "id": company_id, "name": "Role company" },
                            "roleAssignments": {
                                "nodes": if duplicate {
                                    json!([{
                                        "__typename": "CompanyContactRoleAssignment",
                                        "id": format!("gid://shopify/CompanyContactRoleAssignment/{}", 9_886_000 + index),
                                        "companyContact": { "id": id },
                                        "companyLocation": { "id": location_id },
                                        "role": { "id": format!("gid://shopify/CompanyContactRole/{}", 9_885_000 + index) }
                                    }])
                                } else {
                                    json!([])
                                },
                                "pageInfo": {
                                    "hasNextPage": duplicate,
                                    "endCursor": duplicate.then_some("more-unrelated-assignments")
                                }
                            }
                        })
                    }
                    Some(_) if id.starts_with("gid://shopify/CompanyContactRole/") => json!({
                        "__typename": "CompanyContactRole",
                        "id": id,
                        "name": "Ordering only"
                    }),
                    _ if id == location_id => json!({
                        "__typename": "CompanyLocation",
                        "id": location_id,
                        "name": "Role HQ",
                        "company": { "__typename": "Company", "id": company_id, "name": "Role company" }
                    }),
                    _ => Value::Null,
                })
                .collect::<Vec<_>>();
            captured.lock().expect("captured upstream").push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({ "data": { "nodes": nodes } }),
            }
        }
    });

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignManyLocationRoles(
          $locationId: ID!
          $rolesToAssign: [CompanyLocationRoleAssign!]!
        ) {
          companyLocationAssignRoles(
            companyLocationId: $locationId
            rolesToAssign: $rolesToAssign
          ) {
            roleAssignments { id companyContact { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": location_id,
            "rolesToAssign": roles_to_assign
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companyLocationAssignRoles"]["roleAssignments"]
            .as_array()
            .expect("new role assignments")
            .len(),
        12
    );
    assert_eq!(
        response.body["data"]["companyLocationAssignRoles"]["userErrors"]
            .as_array()
            .expect("duplicate role errors")
            .len(),
        13
    );

    let calls = captured.lock().expect("captured upstream");
    assert_eq!(
        calls.len(),
        1,
        "membership probes must share the node batch"
    );
    assert_eq!(calls[0]["variables"]["ids"].as_array().unwrap().len(), 51);
    assert!(calls[0]["query"]
        .as_str()
        .is_some_and(|query| query.contains("roleAssignments(first: 1")
            && !query.contains("B2BContactAssignmentsPageHydrate")));
}

#[test]
fn b2b_companies_count_uses_upstream_total_with_staged_delta() {
    let upstream_responses = Arc::new(Mutex::new(std::collections::VecDeque::from([
        json!({
            "data": {
                "companiesCount": {
                    "count": 40,
                    "precision": "EXACT"
                }
            }
        }),
        json!({
            "data": {
                "companiesCount": {
                    "count": 10,
                    "precision": "AT_LEAST"
                }
            }
        }),
    ])));
    let captured = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        let upstream_responses = Arc::clone(&upstream_responses);
        move |request| {
            captured.lock().expect("captured upstream").push(request);
            Response {
                status: 200,
                headers: Default::default(),
                body: upstream_responses
                    .lock()
                    .expect("upstream responses")
                    .pop_front()
                    .expect("next upstream response"),
            }
        }
    });

    create_b2b_company(&mut proxy, "Staged Buyer");

    let count_only = proxy.process_request(json_graphql_request(
        r#"
        query B2BCompaniesCountOnly {
          companiesCount(limit: null) { count precision }
        }
        "#,
        json!({}),
    ));
    assert_eq!(count_only.status, 200);
    assert_eq!(
        count_only.body["data"]["companiesCount"],
        json!({ "count": 41, "precision": "EXACT" })
    );

    let limited = proxy.process_request(json_graphql_request(
        r#"
        query B2BCompaniesCountLimited {
          companiesCount(limit: 10) { count precision }
        }
        "#,
        json!({}),
    ));
    assert_eq!(limited.status, 200);
    assert_eq!(
        limited.body["data"]["companiesCount"],
        json!({ "count": 10, "precision": "AT_LEAST" })
    );
    assert_eq!(captured.lock().expect("captured upstream").len(), 2);
}

#[test]
fn b2b_companies_count_uses_count_root_not_page_limited_connection() {
    let base_companies = (0..5)
        .map(|index| {
            json!({
                "id": format!("gid://shopify/Company/{}", 700000 + index),
                "name": format!("Baseline Buyer {}", index)
            })
        })
        .collect::<Vec<_>>();
    let upstream_body = json!({
        "data": {
            "companies": {
                "nodes": base_companies,
                "pageInfo": {
                    "hasNextPage": true,
                    "hasPreviousPage": false,
                    "startCursor": "gid://shopify/Company/700000",
                    "endCursor": "gid://shopify/Company/700004"
                }
            },
            "companiesCount": {
                "count": 40,
                "precision": "EXACT"
            }
        }
    });
    let captured = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        let upstream_body = upstream_body.clone();
        move |request| {
            captured.lock().expect("captured upstream").push(request);
            Response {
                status: 200,
                headers: Default::default(),
                body: upstream_body.clone(),
            }
        }
    });

    create_b2b_company(&mut proxy, "Staged Buyer");

    let response = proxy.process_request(json_graphql_request(
        r#"
        query B2BCompaniesSmallPageAndCount {
          companies(first: 5) { nodes { id name } }
          companiesCount(limit: null) { count precision }
        }
        "#,
        json!({}),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companiesCount"],
        json!({ "count": 41, "precision": "EXACT" })
    );
    assert_eq!(
        captured.lock().expect("captured upstream").len(),
        1,
        "one co-selected read should hydrate the page and the independent count"
    );
}

#[test]
fn b2b_live_hybrid_merges_upstream_catalog_with_staged_company_and_location() {
    // Regression: once a B2B row is staged, unrelated upstream rows must stay visible.
    let base_company_id = "gid://shopify/Company/700001";
    let base_location_id = "gid://shopify/CompanyLocation/800001";
    let upstream_body = json!({
        "data": {
            "companies": {
                "nodes": [{
                    "id": base_company_id,
                    "name": "Baseline Buyer",
                    "externalId": "BASE-BUYER",
                    "locations": {
                        "nodes": [{ "id": base_location_id, "name": "Baseline HQ" }]
                    }
                }],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": base_company_id,
                    "endCursor": base_company_id
                }
            },
            "companiesCount": {
                "count": 1,
                "precision": "EXACT"
            },
            "companyLocations": {
                "nodes": [{
                    "id": base_location_id,
                    "name": "Baseline HQ",
                    "externalId": "BASE-HQ",
                    "company": {
                        "id": base_company_id,
                        "name": "Baseline Buyer"
                    }
                }],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": base_location_id,
                    "endCursor": base_location_id
                }
            }
        }
    });
    let captured = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        let upstream_body = upstream_body.clone();
        move |request| {
            assert!(
                !request.body.contains("mutation"),
                "B2B supported mutations must not be forwarded upstream: {}",
                request.body
            );
            captured.lock().expect("captured upstream").push(request);
            Response {
                status: 200,
                headers: Default::default(),
                body: upstream_body.clone(),
            }
        }
    });

    let staged_company_id = create_b2b_company(&mut proxy, "Staged Buyer");
    let staged_default_location_id =
        read_b2b_company_location_ids(&mut proxy, &staged_company_id)[0].clone();
    let staged_location_id = create_b2b_location(&mut proxy, &staged_company_id, "Staged HQ");
    assert!(
        captured.lock().expect("captured upstream").is_empty(),
        "B2B writes should stage without upstream calls"
    );

    let merged = proxy.process_request(json_graphql_request(
        r#"
        query B2BMixedCatalog($companyQuery: String!, $locationQuery: String!) {
          companies(first: 10, query: $companyQuery, sortKey: NAME) {
            nodes {
              id
              name
              externalId
              locations(first: 10, sortKey: NAME) { nodes { id name } }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          companiesCount(limit: 10) { count precision }
          companyLocations(first: 10, query: $locationQuery, sortKey: NAME) {
            nodes { id name externalId company { id name } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "companyQuery": "name:Buyer", "locationQuery": "name:HQ" }),
    ));
    assert_eq!(merged.status, 200);
    assert_eq!(
        merged.body["data"]["companies"]["nodes"],
        json!([
            {
                "id": base_company_id,
                "name": "Baseline Buyer",
                "externalId": "BASE-BUYER",
                "locations": {
                    "nodes": [{ "id": base_location_id, "name": "Baseline HQ" }]
                }
            },
            {
                "id": staged_company_id,
                "name": "Staged Buyer",
                "externalId": Value::Null,
                "locations": {
                    "nodes": [
                        { "id": staged_default_location_id, "name": "Staged Buyer" },
                        { "id": staged_location_id, "name": "Staged HQ" }
                    ]
                }
            }
        ])
    );
    assert_eq!(
        merged.body["data"]["companies"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": base_company_id,
            "endCursor": staged_company_id
        })
    );
    assert_eq!(
        merged.body["data"]["companiesCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
    assert_eq!(
        merged.body["data"]["companyLocations"]["nodes"],
        json!([
            {
                "id": base_location_id,
                "name": "Baseline HQ",
                "externalId": "BASE-HQ",
                "company": { "id": base_company_id, "name": "Baseline Buyer" }
            },
            {
                "id": staged_location_id,
                "name": "Staged HQ",
                "externalId": Value::Null,
                "company": { "id": staged_company_id, "name": "Staged Buyer" }
            }
        ])
    );
    assert_eq!(
        merged.body["data"]["companyLocations"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": base_location_id,
            "endCursor": staged_location_id
        })
    );

    let calls = captured.lock().expect("captured upstream");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].body.contains("query B2BMixedCatalog"));
    assert!(calls.iter().all(|call| !call.body.contains("mutation")));
}

#[test]
fn b2b_count_only_live_hybrid_preserves_upstream_total_with_staged_delta() {
    let captured = Arc::new(Mutex::new(Vec::<Request>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            assert!(
                !request.body.contains("mutation"),
                "B2B supported mutations must not be forwarded upstream: {}",
                request.body
            );
            captured.lock().expect("captured upstream").push(request);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "companiesCount": {
                            "count": 2,
                            "precision": "EXACT"
                        }
                    }
                }),
            }
        }
    });

    let staged_company_id = create_b2b_company(&mut proxy, "Count Only Buyer");
    assert!(staged_company_id.contains("shopify-draft-proxy=synthetic"));
    assert!(
        captured.lock().expect("captured upstream").is_empty(),
        "companyCreate should stage without upstream calls"
    );

    let count = proxy.process_request(json_graphql_request(
        r#"
        query B2BCountOnly {
          companiesCount { count precision }
        }
        "#,
        json!({}),
    ));

    assert_eq!(count.status, 200);
    assert_eq!(
        count.body["data"]["companiesCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );
    let calls = captured.lock().expect("captured upstream");
    assert_eq!(calls.len(), 1);
    assert!(calls[0].body.contains("query B2BCountOnly"));
}

#[test]
fn b2b_live_hybrid_overlays_updates_and_deletes_on_hydrated_baseline() {
    let updated_company_id = "gid://shopify/Company/710001";
    let deleted_company_id = "gid://shopify/Company/710002";
    let updated_location_id = "gid://shopify/CompanyLocation/810001";
    let deleted_location_id = "gid://shopify/CompanyLocation/810002";
    let deleted_company_location_id = "gid://shopify/CompanyLocation/810003";
    let upstream_body = json!({
        "data": {
            "companies": {
                "nodes": [
                    {
                        "id": updated_company_id,
                        "name": "Baseline Buyer",
                        "externalId": "BASE-BUYER",
                        "locations": {
                            "nodes": [
                                {
                                    "id": deleted_location_id,
                                    "name": "Baseline Annex",
                                    "externalId": "BASE-ANNEX"
                                },
                                {
                                    "id": updated_location_id,
                                    "name": "Baseline HQ",
                                    "externalId": "BASE-HQ"
                                }
                            ]
                        }
                    },
                    {
                        "id": deleted_company_id,
                        "name": "Delete Buyer",
                        "externalId": "DELETE-BUYER",
                        "locations": {
                            "nodes": [{
                                "id": deleted_company_location_id,
                                "name": "Delete HQ",
                                "externalId": "DELETE-HQ"
                            }]
                        }
                    }
                ],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": updated_company_id,
                    "endCursor": deleted_company_id
                }
            },
            "companiesCount": {
                "count": 2,
                "precision": "EXACT"
            },
            "companyLocations": {
                "nodes": [
                    {
                        "id": deleted_location_id,
                        "name": "Baseline Annex",
                        "externalId": "BASE-ANNEX",
                        "company": {
                            "id": updated_company_id,
                            "name": "Baseline Buyer"
                        }
                    },
                    {
                        "id": updated_location_id,
                        "name": "Baseline HQ",
                        "externalId": "BASE-HQ",
                        "company": {
                            "id": updated_company_id,
                            "name": "Baseline Buyer"
                        }
                    },
                    {
                        "id": deleted_company_location_id,
                        "name": "Delete HQ",
                        "externalId": "DELETE-HQ",
                        "company": {
                            "id": deleted_company_id,
                            "name": "Delete Buyer"
                        }
                    }
                ],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": deleted_location_id,
                    "endCursor": deleted_company_location_id
                }
            }
        }
    });
    let captured = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        let upstream_body = upstream_body.clone();
        move |request| {
            assert!(
                !request.body.contains("mutation"),
                "B2B supported mutations must not be forwarded upstream: {}",
                request.body
            );
            captured.lock().expect("captured upstream").push(request);
            Response {
                status: 200,
                headers: Default::default(),
                body: upstream_body.clone(),
            }
        }
    });

    let marker_company_id = create_b2b_company(&mut proxy, "Local Hydration Marker");
    let hydrate = proxy.process_request(json_graphql_request(
        r#"
        query B2BHydrateBaselineCatalog {
          companies(first: 10, sortKey: NAME) {
            nodes {
              id
              name
              externalId
              locations(first: 10, sortKey: NAME) { nodes { id name externalId } }
            }
          }
          companiesCount(limit: 10) { count precision }
          companyLocations(first: 10, sortKey: NAME) {
            nodes { id name externalId company { id name } }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(hydrate.status, 200);
    assert_eq!(captured.lock().expect("captured upstream").len(), 1);

    let delete_marker = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BDeleteHydrationMarker($id: ID!) {
          companyDelete(id: $id) {
            deletedCompanyId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": marker_company_id }),
    ));
    assert_eq!(delete_marker.status, 200);
    assert_eq!(
        delete_marker.body["data"]["companyDelete"]["userErrors"],
        json!([])
    );

    let update_company = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BUpdateHydratedCompany($companyId: ID!) {
          companyUpdate(
            companyId: $companyId,
            input: { name: "Updated Buyer", externalId: "UPDATED-BUYER" }
          ) {
            company {
              id
              name
              externalId
              locations(first: 10, sortKey: NAME) { nodes { id name externalId } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": updated_company_id }),
    ));
    assert_eq!(update_company.status, 200);
    assert_eq!(
        update_company.body["data"]["companyUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update_company.body["data"]["companyUpdate"]["company"]["name"],
        json!("Updated Buyer")
    );

    let update_location = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BUpdateHydratedLocation($locationId: ID!) {
          companyLocationUpdate(
            companyLocationId: $locationId,
            input: { name: "Updated HQ", externalId: "UPDATED-HQ" }
          ) {
            companyLocation { id name externalId company { id name } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": updated_location_id }),
    ));
    assert_eq!(update_location.status, 200);
    assert_eq!(
        update_location.body["data"]["companyLocationUpdate"],
        json!({
            "companyLocation": {
                "id": updated_location_id,
                "name": "Updated HQ",
                "externalId": "UPDATED-HQ",
                "company": {
                    "id": updated_company_id,
                    "name": "Updated Buyer"
                }
            },
            "userErrors": []
        })
    );

    let delete_location = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BDeleteHydratedLocation($locationId: ID!) {
          companyLocationDelete(companyLocationId: $locationId) {
            deletedCompanyLocationId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": deleted_location_id }),
    ));
    assert_eq!(delete_location.status, 200);
    assert_eq!(
        delete_location.body["data"]["companyLocationDelete"],
        json!({
            "deletedCompanyLocationId": deleted_location_id,
            "userErrors": []
        })
    );

    let delete_company = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BDeleteHydratedCompany($companyId: ID!) {
          companyDelete(id: $companyId) {
            deletedCompanyId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": deleted_company_id }),
    ));
    assert_eq!(delete_company.status, 200);
    assert_eq!(
        delete_company.body["data"]["companyDelete"],
        json!({
            "deletedCompanyId": deleted_company_id,
            "userErrors": []
        })
    );
    let calls_after_mutations = captured.lock().expect("captured upstream");
    assert_eq!(calls_after_mutations.len(), 7);
    let operation_names = calls_after_mutations
        .iter()
        .filter_map(|call| serde_json::from_str::<Value>(&call.body).ok())
        .filter_map(|body| body["operationName"].as_str().map(str::to_string))
        .collect::<Vec<_>>();
    assert_eq!(
        operation_names
            .iter()
            .filter(|name| name.as_str() == "B2BMutationTargetsHydrate")
            .count(),
        4
    );
    assert_eq!(
        operation_names
            .iter()
            .filter(|name| name.as_str() == "B2BMutationSearchHydrate")
            .count(),
        2
    );
    drop(calls_after_mutations);

    let read_after_mutations = proxy.process_request(json_graphql_request(
        r#"
        query B2BBaselineOverlayReadAfterMutations(
          $updatedCompanyId: ID!,
          $deletedCompanyId: ID!,
          $updatedLocationId: ID!,
          $deletedLocationId: ID!,
          $deletedCompanyLocationId: ID!
        ) {
          updatedCompany: company(id: $updatedCompanyId) {
            id
            name
            externalId
            locations(first: 10, sortKey: NAME) { nodes { id name externalId } }
          }
          deletedCompany: company(id: $deletedCompanyId) { id name }
          updatedLocation: companyLocation(id: $updatedLocationId) {
            id
            name
            externalId
            company { id name }
          }
          deletedLocation: companyLocation(id: $deletedLocationId) { id name }
          deletedCompanyLocation: companyLocation(id: $deletedCompanyLocationId) { id name }
          companies(first: 10, sortKey: NAME) {
            nodes {
              id
              name
              externalId
              locations(first: 10, sortKey: NAME) { nodes { id name } }
            }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          companiesCount(limit: 10) { count precision }
          companyLocations(first: 10, sortKey: NAME) {
            nodes { id name externalId company { id name } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({
            "updatedCompanyId": updated_company_id,
            "deletedCompanyId": deleted_company_id,
            "updatedLocationId": updated_location_id,
            "deletedLocationId": deleted_location_id,
            "deletedCompanyLocationId": deleted_company_location_id
        }),
    ));
    assert_eq!(read_after_mutations.status, 200);
    assert_eq!(
        read_after_mutations.body["data"]["updatedCompany"],
        json!({
            "id": updated_company_id,
            "name": "Updated Buyer",
            "externalId": "UPDATED-BUYER",
            "locations": {
                "nodes": [{
                    "id": updated_location_id,
                    "name": "Updated HQ",
                    "externalId": "UPDATED-HQ"
                }]
            }
        })
    );
    assert!(read_after_mutations.body["data"]["deletedCompany"].is_null());
    assert_eq!(
        read_after_mutations.body["data"]["updatedLocation"],
        json!({
            "id": updated_location_id,
            "name": "Updated HQ",
            "externalId": "UPDATED-HQ",
            "company": {
                "id": updated_company_id,
                "name": "Updated Buyer"
            }
        })
    );
    assert!(read_after_mutations.body["data"]["deletedLocation"].is_null());
    assert!(read_after_mutations.body["data"]["deletedCompanyLocation"].is_null());
    assert_eq!(
        read_after_mutations.body["data"]["companies"]["nodes"],
        json!([{
            "id": updated_company_id,
            "name": "Updated Buyer",
            "externalId": "UPDATED-BUYER",
            "locations": {
                "nodes": [{ "id": updated_location_id, "name": "Updated HQ" }]
            }
        }])
    );
    assert_eq!(
        read_after_mutations.body["data"]["companies"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": updated_company_id,
            "endCursor": updated_company_id
        })
    );
    assert_eq!(
        read_after_mutations.body["data"]["companiesCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read_after_mutations.body["data"]["companyLocations"]["nodes"],
        json!([{
            "id": updated_location_id,
            "name": "Updated HQ",
            "externalId": "UPDATED-HQ",
            "company": {
                "id": updated_company_id,
                "name": "Updated Buyer"
            }
        }])
    );
    assert_eq!(
        read_after_mutations.body["data"]["companyLocations"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": updated_location_id,
            "endCursor": updated_location_id
        })
    );

    let calls = captured.lock().expect("captured upstream");
    assert_eq!(calls.len(), 8);
    assert!(calls[0].body.contains("query B2BHydrateBaselineCatalog"));
    assert!(calls[7]
        .body
        .contains("query B2BBaselineOverlayReadAfterMutations"));
    assert!(calls.iter().all(|call| !call.body.contains("mutation")));
}

#[test]
fn b2b_live_hybrid_stage_locally_roots_do_not_forward_mutations() {
    let captured = Arc::new(Mutex::new(Vec::<Value>::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            let body = serde_json::from_str::<Value>(&request.body).expect("upstream body");
            assert!(
                !body["query"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("mutation"),
                "B2B staged mutations may issue only read-only prerequisite queries"
            );
            assert_eq!(body["operationName"], json!("B2BMutationSearchHydrate"));
            captured.lock().expect("captured upstream").push(body);
            Response {
                status: 200,
                headers: Default::default(),
                body: json!({
                    "data": {
                        "companies": { "nodes": [] },
                        "companyLocations": { "nodes": [] },
                        "customers": { "nodes": [] }
                    }
                }),
            }
        }
    });

    let company_id = create_b2b_company(&mut proxy, "Live Local B2B Co");
    let contact_id = create_b2b_company_contact(&mut proxy, &company_id, "Initial buyer");

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLiveLocalContactUpdate($companyContactId: ID!, $input: CompanyContactInput!) {
          companyContactUpdate(companyContactId: $companyContactId, input: $input) {
            companyContact { id title customer { firstName lastName email } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": contact_id,
            "input": {
                "title": "Updated buyer",
                "firstName": "Local",
                "lastName": "Buyer",
                "email": "local-buyer@example.test"
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["companyContactUpdate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        update.body["data"]["companyContactUpdate"]["companyContact"]["title"],
        json!("Updated buyer")
    );

    let assign_main = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLiveLocalAssignMain($companyId: ID!, $companyContactId: ID!) {
          companyAssignMainContact(companyId: $companyId, companyContactId: $companyContactId) {
            company { id mainContact { id title isMainContact } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id, "companyContactId": contact_id }),
    ));
    assert_eq!(
        assign_main.body["data"]["companyAssignMainContact"]["company"]["mainContact"],
        json!({ "id": contact_id, "title": "Updated buyer", "isMainContact": true })
    );

    let revoke_main = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLiveLocalRevokeMain($companyId: ID!) {
          companyRevokeMainContact(companyId: $companyId) {
            company { id mainContact { id } contacts(first: 5) { nodes { id isMainContact } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id }),
    ));
    assert_eq!(
        revoke_main.body["data"]["companyRevokeMainContact"]["company"]["mainContact"],
        Value::Null
    );
    assert_eq!(
        revoke_main.body["data"]["companyRevokeMainContact"]["company"]["contacts"]["nodes"][0]
            ["isMainContact"],
        json!(false)
    );

    let location_id = create_b2b_location_with_shared_address(&mut proxy, &company_id);
    let location = proxy.process_request(json_graphql_request(
        r#"
        query B2BLiveLocalSharedAddress($id: ID!) {
          companyLocation(id: $id) {
            shippingAddress { id }
            billingAddress { id }
          }
        }
        "#,
        json!({ "id": location_id }),
    ));
    let address_id = location.body["data"]["companyLocation"]["shippingAddress"]["id"]
        .as_str()
        .expect("address id")
        .to_string();
    assert_eq!(
        location.body["data"]["companyLocation"]["billingAddress"]["id"],
        json!(address_id)
    );

    let delete_address = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLiveLocalAddressDelete($addressId: ID!) {
          companyAddressDelete(addressId: $addressId) {
            deletedAddressId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "addressId": address_id }),
    ));
    assert_eq!(
        delete_address.body["data"]["companyAddressDelete"],
        json!({ "deletedAddressId": address_id, "userErrors": [] })
    );

    let delete_contact = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLiveLocalContactDelete($companyContactId: ID!) {
          companyContactDelete(companyContactId: $companyContactId) {
            deletedCompanyContactId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyContactId": contact_id }),
    ));
    assert_eq!(
        delete_contact.body["data"]["companyContactDelete"],
        json!({ "deletedCompanyContactId": contact_id, "userErrors": [] })
    );
    assert_eq!(captured.lock().expect("captured upstream").len(), 2);
}

#[test]
fn b2b_company_location_update_hydrates_real_upstream_location() {
    let location_id = "gid://shopify/CompanyLocation/987654321";
    let company_id = "gid://shopify/Company/456789";
    let captured = Arc::new(Mutex::new(Vec::new()));
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let captured = Arc::clone(&captured);
        move |request| {
            captured
                .lock()
                .expect("captured upstream")
                .push(serde_json::from_str::<Value>(&request.body).expect("upstream body"));
            b2b_mutation_targets_location_hydrate_response(location_id, "Hydrated HQ", company_id)
        }
    });

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BHydratedLocationUpdate($id: ID!) {
          companyLocationUpdate(
            companyLocationId: $id,
            input: {
              name: "Updated Real HQ",
              buyerExperienceConfiguration: {
                paymentTermsTemplateId: "gid://shopify/PaymentTermsTemplate/4",
                checkoutToDraft: true,
                editableShippingAddress: true
              }
            }
          ) {
            companyLocation {
              id
              name
              taxSettings { taxExempt taxExemptions }
              buyerExperienceConfiguration {
                checkoutToDraft
                editableShippingAddress
                paymentTermsTemplate { id }
              }
              company { id name }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["companyLocationUpdate"],
        json!({
            "companyLocation": {
                "id": location_id,
                "name": "Updated Real HQ",
                "taxSettings": {
                    "taxExempt": false,
                    "taxExemptions": ["US_CA_RESELLER_EXEMPTION"]
                },
                "buyerExperienceConfiguration": {
                    "checkoutToDraft": true,
                    "editableShippingAddress": true,
                    "paymentTermsTemplate": { "id": "gid://shopify/PaymentTermsTemplate/4" }
                },
                "company": {
                    "id": company_id,
                    "name": "Hydrated Company"
                }
            },
            "userErrors": []
        })
    );

    let readback = proxy.process_request(json_graphql_request(
        r#"
        query B2BHydratedLocationRead($id: ID!) {
          companyLocation(id: $id) {
            id
            name
            taxSettings { taxExempt taxExemptions }
          }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(
        readback.body["data"]["companyLocation"],
        json!({
            "id": location_id,
            "name": "Updated Real HQ",
            "taxSettings": {
                "taxExempt": false,
                "taxExemptions": ["US_CA_RESELLER_EXEMPTION"]
            }
        })
    );

    let calls = captured.lock().expect("captured upstream");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0]["operationName"],
        json!("B2BMutationTargetsHydrate")
    );
    assert_eq!(calls[0]["variables"]["ids"], json!([location_id]));
    let hydrate_query = calls[0]["query"].as_str().expect("hydrate query");
    assert!(hydrate_query.contains("nodes(ids: $ids)"));
    assert!(!hydrate_query.contains("billingSameAsShipping"));
}

#[test]
fn b2b_tax_settings_and_store_credit_hydrate_real_company_locations() {
    let tax_location_id = "gid://shopify/CompanyLocation/987654322";
    let credit_location_id = "gid://shopify/CompanyLocation/987654323";
    let company_id = "gid://shopify/Company/456790";

    let tax_calls = Arc::new(Mutex::new(Vec::new()));
    let mut tax_proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let tax_calls = Arc::clone(&tax_calls);
        move |request| {
            tax_calls
                .lock()
                .expect("captured tax upstream")
                .push(serde_json::from_str::<Value>(&request.body).expect("upstream body"));
            b2b_mutation_targets_location_hydrate_response(
                tax_location_id,
                "Tax Hydrated",
                company_id,
            )
        }
    });
    let tax = tax_proxy.process_request(json_graphql_request(
        r#"
        mutation B2BHydratedLocationTax($id: ID!) {
          companyLocationTaxSettingsUpdate(
            companyLocationId: $id,
            taxRegistrationId: "REG-LIVE",
            taxExempt: true
          ) {
            companyLocation { id name taxSettings { taxRegistrationId taxExempt taxExemptions } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": tax_location_id }),
    ));
    assert_eq!(
        tax.body["data"]["companyLocationTaxSettingsUpdate"],
        json!({
            "companyLocation": {
                "id": tax_location_id,
                "name": "Tax Hydrated",
                "taxSettings": {
                    "taxRegistrationId": "REG-LIVE",
                    "taxExempt": true,
                    "taxExemptions": ["US_CA_RESELLER_EXEMPTION"]
                }
            },
            "userErrors": []
        })
    );
    assert_eq!(tax_calls.lock().expect("tax calls").len(), 1);

    let credit_calls = Arc::new(Mutex::new(Vec::new()));
    let mut credit_proxy = configured_proxy(ReadMode::LiveHybrid, None).with_upstream_transport({
        let credit_calls = Arc::clone(&credit_calls);
        move |request| {
            credit_calls
                .lock()
                .expect("captured credit upstream")
                .push(serde_json::from_str::<Value>(&request.body).expect("upstream body"));
            b2b_company_location_hydrate_response(credit_location_id, "Credit Hydrated", company_id)
        }
    });
    let credit = credit_proxy.process_request(json_graphql_request(
        r#"
        mutation B2BHydratedLocationStoreCredit($id: ID!) {
          storeCreditAccountCredit(id: $id, creditInput: { creditAmount: { amount: "3.00", currencyCode: USD } }) {
            storeCreditAccountTransaction {
              account {
                balance { amount currencyCode }
                owner { ... on CompanyLocation { id name } }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": credit_location_id }),
    ));
    assert_eq!(
        credit.body["data"]["storeCreditAccountCredit"]["userErrors"],
        json!([])
    );
    assert_eq!(
        credit.body["data"]["storeCreditAccountCredit"]["storeCreditAccountTransaction"]["account"]
            ["owner"],
        json!({ "id": credit_location_id, "name": "Credit Hydrated" })
    );
    assert_eq!(credit_calls.lock().expect("credit calls").len(), 1);
}

#[test]
fn b2b_running_mutations_return_resource_specific_not_found_messages_without_staging() {
    let mut proxy = snapshot_proxy();
    let unknown_company_id = "gid://shopify/Company/999999999999";
    let unknown_address_id = "gid://shopify/CompanyAddress/999999999999";
    let unknown_location_id = "gid://shopify/CompanyLocation/999999999999";

    let company_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompanyDeleteUnknown($id: ID!) {
          companyDelete(id: $id) {
            deletedCompanyId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": unknown_company_id }),
    ));
    assert_eq!(company_delete.status, 200);
    assert_eq!(
        company_delete.body["data"]["companyDelete"],
        json!({
            "deletedCompanyId": Value::Null,
            "userErrors": [{
                "field": ["id"],
                "message": "Company does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let contact_create = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactCreateUnknownCompany($companyId: ID!) {
          companyContactCreate(companyId: $companyId, input: { title: "Buyer", email: "buyer@example.test" }) {
            companyContact { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": unknown_company_id }),
    ));
    assert_eq!(contact_create.status, 200);
    assert_eq!(
        contact_create.body["data"]["companyContactCreate"],
        json!({
            "companyContact": Value::Null,
            "userErrors": [{
                "field": ["companyId"],
                "message": "Company does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let address_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAddressDeleteUnknown($addressId: ID!) {
          companyAddressDelete(addressId: $addressId) {
            deletedAddressId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "addressId": unknown_address_id }),
    ));
    assert_eq!(address_delete.status, 200);
    assert_eq!(
        address_delete.body["data"]["companyAddressDelete"],
        json!({
            "deletedAddressId": Value::Null,
            "userErrors": [{
                "field": ["addressId"],
                "message": "Company address was not found.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let assign_roles = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationAssignRolesUnknown($locationId: ID!) {
          companyLocationAssignRoles(companyLocationId: $locationId, rolesToAssign: []) {
            roleAssignments { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": unknown_location_id }),
    ));
    assert_eq!(assign_roles.status, 200);
    assert_eq!(
        assign_roles.body["data"]["companyLocationAssignRoles"],
        json!({
            "roleAssignments": Value::Null,
            "userErrors": [{
                "field": ["companyLocationId"],
                "message": "Location does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let revoke_roles = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationRevokeRolesUnknown($locationId: ID!) {
          companyLocationRevokeRoles(companyLocationId: $locationId, rolesToRevoke: []) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": unknown_location_id }),
    ));
    assert_eq!(revoke_roles.status, 200);
    assert_eq!(
        revoke_roles.body["data"]["companyLocationRevokeRoles"],
        json!({
            "revokedRoleAssignmentIds": Value::Null,
            "userErrors": [{
                "field": ["companyLocationId"],
                "message": "Location does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let entries = log_snapshot(&proxy)["entries"]
        .as_array()
        .expect("log entries")
        .clone();
    for root in [
        "companyDelete",
        "companyContactCreate",
        "companyAddressDelete",
        "companyLocationAssignRoles",
        "companyLocationRevokeRoles",
    ] {
        let entry = entries
            .iter()
            .find(|entry| entry["interpreted"]["primaryRootField"] == json!(root))
            .unwrap_or_else(|| panic!("missing {root} log entry"));
        assert_eq!(entry["status"], json!("failed"));
        assert_eq!(entry["stagedResourceIds"], json!([]));
    }
}

#[test]
fn b2b_company_contact_lifecycle_and_main_contact_stage_locally() {
    let mut proxy = snapshot_proxy();

    let company = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyContactLifecycleCompany($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id name mainContact { id } contacts(first: 5) { nodes { id } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Contact Lifecycle Co" } }),
    ));
    assert_eq!(company.status, 200);
    let company_id = company.body["data"]["companyCreate"]["company"]["id"].clone();

    let create = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactLifecycleCreate($companyId: ID!, $input: CompanyContactInput!) {
          companyContactCreate(companyId: $companyId, input: $input) {
            companyContact {
              id
              title
              isMainContact
              company { id name }
              customer { id email firstName lastName phone }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyId": company_id,
            "input": {
                "firstName": "Ada",
                "lastName": "Lovelace",
                "email": "ada@example.com",
                "phone": "+14155550100",
                "title": "Buyer"
            }
        }),
    ));
    assert_eq!(create.status, 200);
    assert_eq!(
        create.body["data"]["companyContactCreate"]["userErrors"],
        json!([])
    );
    let contact_id = create.body["data"]["companyContactCreate"]["companyContact"]["id"].clone();
    assert_eq!(
        create.body["data"]["companyContactCreate"]["companyContact"]["company"]["id"],
        company_id
    );

    let update = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactLifecycleUpdate($companyContactId: ID!, $input: CompanyContactInput!) {
          companyContactUpdate(companyContactId: $companyContactId, input: $input) {
            companyContact {
              id
              title
              customer {
                firstName
                lastName
                email
                phone
                defaultPhoneNumber { phoneNumber }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": contact_id,
            "input": {
                "firstName": "Grace",
                "lastName": "Hopper",
                "email": "grace@example.com",
                "phone": "(415) 555-0101",
                "title": "Lead buyer"
            }
        }),
    ));
    assert_eq!(update.status, 200);
    assert_eq!(
        update.body["data"]["companyContactUpdate"],
        json!({
            "companyContact": {
                "id": contact_id,
                "title": "Lead buyer",
                "customer": {
                    "firstName": "Grace",
                    "lastName": "Hopper",
                    "email": "grace@example.com",
                    "phone": "+14155550101",
                    "defaultPhoneNumber": {
                        "phoneNumber": "+14155550101"
                    }
                }
            },
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BContactLifecycleRead($id: ID!) {
          companyContact(id: $id) {
            id
            title
            isMainContact
            customer { email firstName lastName phone }
          }
        }
        "#,
        json!({ "id": contact_id }),
    ));
    assert_eq!(
        read.body["data"]["companyContact"],
        json!({
            "id": contact_id,
            "title": "Lead buyer",
            "isMainContact": false,
            "customer": {
                "email": "grace@example.com",
                "firstName": "Grace",
                "lastName": "Hopper",
                "phone": "+14155550101"
            }
        })
    );

    let assign_main = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactLifecycleAssignMain($companyId: ID!, $companyContactId: ID!) {
          companyAssignMainContact(companyId: $companyId, companyContactId: $companyContactId) {
            company {
              id
              mainContact { id title isMainContact }
              contacts(first: 5) { nodes { id title isMainContact } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id, "companyContactId": contact_id }),
    ));
    assert_eq!(
        assign_main.body["data"]["companyAssignMainContact"]["company"]["mainContact"],
        json!({ "id": contact_id, "title": "Lead buyer", "isMainContact": true })
    );

    let revoke_main = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactLifecycleRevokeMain($companyId: ID!) {
          companyRevokeMainContact(companyId: $companyId) {
            company {
              id
              mainContact { id title isMainContact }
              contacts(first: 5) { nodes { id isMainContact } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id }),
    ));
    assert_eq!(
        revoke_main.body["data"]["companyRevokeMainContact"]["company"]["mainContact"],
        Value::Null
    );
    assert_eq!(
        revoke_main.body["data"]["companyRevokeMainContact"]["company"]["contacts"]["nodes"][0]
            ["isMainContact"],
        json!(false)
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactLifecycleDelete($companyContactId: ID!) {
          companyContactDelete(companyContactId: $companyContactId) {
            deletedCompanyContactId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyContactId": contact_id }),
    ));
    assert_eq!(
        delete.body["data"]["companyContactDelete"],
        json!({ "deletedCompanyContactId": contact_id, "userErrors": [] })
    );

    let read_deleted = proxy.process_request(json_graphql_request(
        r#"query B2BContactLifecycleReadDeleted($id: ID!) { companyContact(id: $id) { id } }"#,
        json!({ "id": contact_id }),
    ));
    assert_eq!(read_deleted.body["data"]["companyContact"], Value::Null);

    let entries = log_snapshot(&proxy)["entries"]
        .as_array()
        .expect("log entries")
        .clone();
    for root in [
        "companyContactCreate",
        "companyContactUpdate",
        "companyAssignMainContact",
        "companyRevokeMainContact",
        "companyContactDelete",
    ] {
        let entry = entries
            .iter()
            .find(|entry| entry["interpreted"]["primaryRootField"] == json!(root))
            .unwrap_or_else(|| panic!("missing {root} log entry"));
        assert_eq!(entry["status"], json!("staged"));
        assert!(entry["rawBody"].as_str().unwrap_or_default().contains(root));
    }
}

#[test]
fn b2b_assign_main_contact_distinguishes_cross_company_from_unknown_contact() {
    let mut proxy = snapshot_proxy();
    let source_company_id = create_b2b_company(&mut proxy, "Source Contact Co");
    let target_company_id = create_b2b_company(&mut proxy, "Target Main Co");
    let source_contact_id =
        create_b2b_company_contact(&mut proxy, &source_company_id, "Source Buyer");

    let wrong_company = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignWrongCompanyMainContact($companyId: ID!, $companyContactId: ID!) {
          companyAssignMainContact(companyId: $companyId, companyContactId: $companyContactId) {
            company { id mainContact { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": target_company_id, "companyContactId": source_contact_id }),
    ));
    assert_eq!(wrong_company.status, 200);
    assert_eq!(
        wrong_company.body["data"]["companyAssignMainContact"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["companyContactId"],
                "message": "The company contact does not belong to the company.",
                "code": "INVALID_INPUT"
            }]
        })
    );

    let unknown_contact_id = "gid://shopify/CompanyContact/404?shopify-draft-proxy=synthetic";
    let unknown_contact = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignUnknownMainContact($companyId: ID!, $companyContactId: ID!) {
          companyAssignMainContact(companyId: $companyId, companyContactId: $companyContactId) {
            company { id mainContact { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": target_company_id, "companyContactId": unknown_contact_id }),
    ));
    assert_eq!(unknown_contact.status, 200);
    assert_eq!(
        unknown_contact.body["data"]["companyAssignMainContact"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["companyContactId"],
                "message": "The company contact doesn't exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let target_read = proxy.process_request(json_graphql_request(
        r#"
        query B2BAssignWrongCompanyTargetRead($companyId: ID!) {
          company(id: $companyId) {
            id
            mainContact { id }
          }
        }
        "#,
        json!({ "companyId": target_company_id }),
    ));
    assert_eq!(
        target_read.body["data"]["company"]["mainContact"],
        Value::Null
    );
}

#[test]
fn b2b_contact_validation_and_bulk_delete_use_shopify_field_paths() {
    let mut proxy = snapshot_proxy();
    let company = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyContactValidationCompany($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Contact Validation Co" } }),
    ));
    let company_id = company.body["data"]["companyCreate"]["company"]["id"].clone();

    let html_title = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactValidationHtml($companyId: ID!, $input: CompanyContactInput!) {
          companyContactCreate(companyId: $companyId, input: $input) {
            companyContact { id title }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyId": company_id,
            "input": { "firstName": "Ada", "lastName": "Buyer", "email": "ada@example.com", "title": "<b>VP</b>" }
        }),
    ));
    assert_eq!(
        html_title.body["data"]["companyContactCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        html_title.body["data"]["companyContactCreate"]["companyContact"]["title"],
        json!("<b>VP</b>")
    );

    let html_name = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactValidationHtmlName($companyId: ID!, $input: CompanyContactInput!) {
          companyContactCreate(companyId: $companyId, input: $input) {
            companyContact { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyId": company_id,
            "input": { "firstName": "<b>Ada</b>", "lastName": "Buyer", "email": "ada-html@example.com" }
        }),
    ));
    assert_eq!(
        html_name.body["data"]["companyContactCreate"],
        json!({
            "companyContact": Value::Null,
            "userErrors": [{
                "field": ["input"],
                "message": "Invalid input.",
                "code": "INVALID_INPUT"
            }]
        })
    );

    let long_name = "x".repeat(256);
    let too_long = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactValidationLongName($companyId: ID!, $input: CompanyContactInput!) {
          companyContactCreate(companyId: $companyId, input: $input) {
            companyContact { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyId": company_id,
            "input": { "firstName": long_name, "lastName": "Buyer", "email": "long@example.com" }
        }),
    ));
    assert_eq!(
        too_long.body["data"]["companyContactCreate"]["userErrors"][0],
        json!({
            "field": ["input", "firstName"],
            "message": "First name is too long",
            "code": "TOO_LONG"
        })
    );

    let first = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactValidationGood($companyId: ID!, $input: CompanyContactInput!) {
          companyContactCreate(companyId: $companyId, input: $input) {
            companyContact { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyId": company_id,
            "input": { "firstName": "One", "lastName": "Buyer", "email": "one@example.com" }
        }),
    ));
    let second = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactValidationGood($companyId: ID!, $input: CompanyContactInput!) {
          companyContactCreate(companyId: $companyId, input: $input) {
            companyContact { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyId": company_id,
            "input": { "firstName": "Two", "lastName": "Buyer", "email": "two@example.com" }
        }),
    ));
    let first_id = first.body["data"]["companyContactCreate"]["companyContact"]["id"].clone();
    let second_id = second.body["data"]["companyContactCreate"]["companyContact"]["id"].clone();
    let missing_id = "gid://shopify/CompanyContact/404?shopify-draft-proxy=synthetic";

    let bulk_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactsDeletePaths($companyContactIds: [ID!]!) {
          companyContactsDelete(companyContactIds: $companyContactIds) {
            deletedCompanyContactIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyContactIds": [first_id, missing_id, second_id] }),
    ));
    assert_eq!(
        bulk_delete.body["data"]["companyContactsDelete"]["deletedCompanyContactIds"],
        json!([first_id, second_id])
    );
    assert_eq!(
        bulk_delete.body["data"]["companyContactsDelete"]["userErrors"],
        json!([{
            "field": ["companyContactIds", "1"],
            "message": "The company contact doesn't exist.",
            "code": "RESOURCE_NOT_FOUND"
        }])
    );
}

#[test]
fn b2b_company_contact_create_without_email_rejects_and_stages_nothing() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "Missing Email Contact Co");
    let state_before = state_snapshot(&proxy);

    let rejected = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactCreateMissingEmail($companyId: ID!, $input: CompanyContactInput!) {
          companyContactCreate(companyId: $companyId, input: $input) {
            companyContact { id customer { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyId": company_id,
            "input": {
                "firstName": "Jane",
                "lastName": "Doe"
            }
        }),
    ));
    assert_eq!(rejected.status, 200);
    assert_eq!(
        rejected.body["data"]["companyContactCreate"],
        json!({
            "companyContact": Value::Null,
            "userErrors": [{
                "field": ["input"],
                "message": "Either the attribute email or customer_id must be provided",
                "code": "INVALID"
            }]
        })
    );
    assert_eq!(state_snapshot(&proxy), state_before);

    let b2b_read_after = proxy.process_request(json_graphql_request(
        r#"
        query B2BContactCreateMissingEmailReadAfter($companyId: ID!, $contactId: ID!) {
          companyContact(id: $contactId) { id }
          company(id: $companyId) { contacts(first: 5) { nodes { id } } }
        }
        "#,
        json!({
            "companyId": company_id,
            "contactId": "gid://shopify/CompanyContact/1?shopify-draft-proxy=synthetic"
        }),
    ));
    assert_eq!(b2b_read_after.body["data"]["companyContact"], Value::Null);
    assert_eq!(
        b2b_read_after.body["data"]["company"]["contacts"]["nodes"],
        json!([])
    );

    let customer_read_after = proxy.process_request(json_graphql_request(
        r#"
        query B2BContactCreateMissingEmailCustomerReadAfter($customerId: ID!) {
          customer(id: $customerId) { id }
        }
        "#,
        json!({ "customerId": "gid://shopify/Customer/1?shopify-draft-proxy=synthetic" }),
    ));
    assert_eq!(customer_read_after.body["data"]["customer"], Value::Null);

    let entries = log_snapshot(&proxy)["entries"]
        .as_array()
        .expect("log entries")
        .clone();
    let rejected_entry = entries
        .iter()
        .find(|entry| entry["interpreted"]["primaryRootField"] == json!("companyContactCreate"))
        .expect("companyContactCreate log entry");
    assert_eq!(rejected_entry["status"], json!("failed"));
    assert_eq!(rejected_entry["stagedResourceIds"], json!([]));
}

#[test]
fn b2b_company_create_nested_contact_without_email_is_atomic() {
    let mut proxy = snapshot_proxy();

    let rejected = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompanyCreateNestedContactMissingEmail($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company {
              id
              locations(first: 5) { nodes { id } }
              contacts(first: 5) { nodes { id } }
              contactRoles(first: 5) { nodes { id } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "company": { "name": "Nested Missing Email Co" },
                "companyContact": { "firstName": "Jane" }
            }
        }),
    ));
    assert_eq!(rejected.status, 200);
    assert_eq!(
        rejected.body["data"]["companyCreate"],
        json!({
            "company": Value::Null,
            "userErrors": [{
                "field": ["input", "companyContact"],
                "message": "Either the attribute email or customer_id must be provided",
                "code": "INVALID"
            }]
        })
    );

    let state = state_snapshot(&proxy);
    assert!(state["stagedState"].get("b2bCompanies").is_none());
    assert!(state["stagedState"].get("b2bLocations").is_none());
    assert!(state["stagedState"].get("b2bContacts").is_none());
    assert!(state["stagedState"].get("b2bContactRoles").is_none());
    assert!(state["stagedState"].get("b2bRoleAssignments").is_none());
    assert_eq!(state["stagedState"]["customers"], json!({}));

    let read_after = proxy.process_request(json_graphql_request(
        r#"
        query B2BCompanyCreateNestedContactMissingEmailReadAfter($companyId: ID!) {
          company(id: $companyId) { id }
        }
        "#,
        json!({ "companyId": "gid://shopify/Company/1?shopify-draft-proxy=synthetic" }),
    ));
    assert_eq!(read_after.body["data"]["company"], Value::Null);

    let entries = log_snapshot(&proxy)["entries"]
        .as_array()
        .expect("log entries")
        .clone();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0]["interpreted"]["primaryRootField"],
        json!("companyCreate")
    );
    assert_eq!(entries[0]["status"], json!("failed"));
    assert_eq!(entries[0]["stagedResourceIds"], json!([]));
}

#[test]
fn b2b_contact_role_assign_and_revoke_stage_relationships_with_indexed_errors() {
    let mut proxy = snapshot_proxy();
    let company = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyContactRolesCompany($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company {
              id
              contactRoles(first: 5) { nodes { id name } }
              locations(first: 5) { nodes { id } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Contact Roles Co" } }),
    ));
    let company_id = company.body["data"]["companyCreate"]["company"]["id"].clone();
    let role_id =
        company.body["data"]["companyCreate"]["company"]["contactRoles"]["nodes"][0]["id"].clone();
    // companyCreate provisions a default location; assign the role against it
    // rather than a hardcoded synthetic id so the test tracks the real allocation.
    let location_id =
        company.body["data"]["companyCreate"]["company"]["locations"]["nodes"][0]["id"].clone();

    let contact = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactRolesContact($companyId: ID!, $input: CompanyContactInput!) {
          companyContactCreate(companyId: $companyId, input: $input) {
            companyContact { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyId": company_id,
            "input": { "firstName": "Role", "lastName": "Buyer", "email": "role@example.com", "title": "Buyer" }
        }),
    ));
    let contact_id = contact.body["data"]["companyContactCreate"]["companyContact"]["id"].clone();

    let assign = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignRole($companyContactId: ID!, $companyContactRoleId: ID!, $companyLocationId: ID!) {
          companyContactAssignRole(
            companyContactId: $companyContactId
            companyContactRoleId: $companyContactRoleId
            companyLocationId: $companyLocationId
          ) {
            companyContactRoleAssignment {
              id
              companyContact { id title }
              role { id name }
              companyLocation { id name }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": contact_id,
            "companyContactRoleId": role_id,
            "companyLocationId": location_id
        }),
    ));
    assert_eq!(
        assign.body["data"]["companyContactAssignRole"]["userErrors"],
        json!([])
    );
    let assignment_id = assign.body["data"]["companyContactAssignRole"]
        ["companyContactRoleAssignment"]["id"]
        .clone();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BContactRolesRead($companyContactId: ID!, $companyLocationId: ID!) {
          companyContact(id: $companyContactId) {
            id
            roleAssignments(first: 5) { nodes { id role { id name } companyLocation { id name } } }
          }
          companyLocation(id: $companyLocationId) {
            id
            roleAssignments(first: 5) { nodes { id companyContact { id title } } }
          }
        }
        "#,
        json!({ "companyContactId": contact_id, "companyLocationId": location_id }),
    ));
    assert_eq!(
        read.body["data"]["companyContact"]["roleAssignments"]["nodes"][0]["id"],
        assignment_id
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["roleAssignments"]["nodes"][0]["id"],
        assignment_id
    );

    let bulk_assign_errors = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignRolesPaths($companyContactId: ID!, $rolesToAssign: [CompanyContactRoleAssign!]!) {
          companyContactAssignRoles(companyContactId: $companyContactId, rolesToAssign: $rolesToAssign) {
            roleAssignments { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": contact_id,
            "rolesToAssign": [
                { "companyContactRoleId": role_id, "companyLocationId": "gid://shopify/CompanyLocation/404?shopify-draft-proxy=synthetic" },
                { "companyContactRoleId": "gid://shopify/CompanyContactRole/404?shopify-draft-proxy=synthetic", "companyLocationId": location_id }
            ]
        }),
    ));
    assert_eq!(
        bulk_assign_errors.body["data"]["companyContactAssignRoles"]["roleAssignments"],
        json!([])
    );
    assert_eq!(
        bulk_assign_errors.body["data"]["companyContactAssignRoles"]["userErrors"],
        json!([
            {
                "field": ["rolesToAssign", "0", "companyLocationId"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            },
            {
                "field": ["rolesToAssign", "1", "companyContactRoleId"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }
        ])
    );

    let revoke_errors = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BRevokeRolesPaths($companyContactId: ID!, $roleAssignmentIds: [ID!]!) {
          companyContactRevokeRoles(companyContactId: $companyContactId, roleAssignmentIds: $roleAssignmentIds) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": contact_id,
            "roleAssignmentIds": [assignment_id, "gid://shopify/CompanyContactRoleAssignment/404?shopify-draft-proxy=synthetic"]
        }),
    ));
    assert_eq!(
        revoke_errors.body["data"]["companyContactRevokeRoles"]["revokedRoleAssignmentIds"],
        json!([assignment_id])
    );
    assert_eq!(
        revoke_errors.body["data"]["companyContactRevokeRoles"]["userErrors"],
        json!([{
            "field": ["roleAssignmentIds", "1"],
            "message": "Resource requested does not exist.",
            "code": "RESOURCE_NOT_FOUND"
        }])
    );
}

#[test]
fn b2b_contact_assign_role_checks_location_before_role_for_missing_resources() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company_with_contact_and_role(&mut proxy);
    let company = proxy.process_request(json_graphql_request(
        r#"
        query B2BContactAssignRoleOrderingSetup($id: ID!) {
          company(id: $id) {
            contacts(first: 5) { nodes { id } }
            contactRoles(first: 5) { nodes { id } }
            locations(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "id": company_id }),
    ));
    assert_eq!(company.status, 200);
    let contact_id = company.body["data"]["company"]["contacts"]["nodes"][0]["id"]
        .as_str()
        .expect("contact id")
        .to_string();
    let role_id = company.body["data"]["company"]["contactRoles"]["nodes"][0]["id"]
        .as_str()
        .expect("role id")
        .to_string();
    let location_id = company.body["data"]["company"]["locations"]["nodes"][0]["id"]
        .as_str()
        .expect("location id")
        .to_string();
    let missing_role_id = "gid://shopify/CompanyContactRole/999999999999999";
    let missing_location_id = "gid://shopify/CompanyLocation/999999999999999";

    let mut assign_errors = |role_id: &str, location_id: &str| {
        let response = proxy.process_request(json_graphql_request(
            r#"
            mutation B2BContactAssignRoleMissingOrdering(
              $companyContactId: ID!
              $companyContactRoleId: ID!
              $companyLocationId: ID!
            ) {
              companyContactAssignRole(
                companyContactId: $companyContactId
                companyContactRoleId: $companyContactRoleId
                companyLocationId: $companyLocationId
              ) {
                companyContactRoleAssignment { id }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "companyContactId": contact_id,
                "companyContactRoleId": role_id,
                "companyLocationId": location_id
            }),
        ));
        assert_eq!(response.status, 200);
        assert_eq!(
            response.body["data"]["companyContactAssignRole"]["companyContactRoleAssignment"],
            Value::Null
        );
        response.body["data"]["companyContactAssignRole"]["userErrors"].clone()
    };

    assert_eq!(
        assign_errors(missing_role_id, &location_id),
        json!([{
            "field": ["companyContactRoleId"],
            "message": "The company contact role doesn't exist.",
            "code": "RESOURCE_NOT_FOUND"
        }])
    );
    assert_eq!(
        assign_errors(&role_id, missing_location_id),
        json!([{
            "field": ["companyLocationId"],
            "message": "The company location doesn't exist.",
            "code": "RESOURCE_NOT_FOUND"
        }])
    );
    assert_eq!(
        assign_errors(missing_role_id, missing_location_id),
        json!([{
            "field": ["companyLocationId"],
            "message": "The company location doesn't exist.",
            "code": "RESOURCE_NOT_FOUND"
        }])
    );
}

#[test]
fn b2b_bulk_role_assign_rejects_duplicate_contact_location_and_keeps_valid_siblings() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company_with_contact_and_role(&mut proxy);
    let first_location_id = create_b2b_location(&mut proxy, &company_id, "Duplicate HQ");
    let second_location_id = create_b2b_location(&mut proxy, &company_id, "Valid HQ");
    let second_contact_id = create_b2b_company_contact(&mut proxy, &company_id, "Second Buyer");
    let company = proxy.process_request(json_graphql_request(
        r#"
        query B2BBulkDuplicateRoleSetup($id: ID!) {
          company(id: $id) {
            contacts(first: 5) { nodes { id } }
            contactRoles(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "id": company_id }),
    ));
    let first_contact_id = company.body["data"]["company"]["contacts"]["nodes"][0]["id"]
        .as_str()
        .expect("contact id")
        .to_string();
    let role_id = company.body["data"]["company"]["contactRoles"]["nodes"][0]["id"]
        .as_str()
        .expect("role id")
        .to_string();

    let initial_assign = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BBulkDuplicateSeedAssign(
          $companyContactId: ID!
          $companyContactRoleId: ID!
          $companyLocationId: ID!
        ) {
          companyContactAssignRole(
            companyContactId: $companyContactId
            companyContactRoleId: $companyContactRoleId
            companyLocationId: $companyLocationId
          ) {
            companyContactRoleAssignment { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": first_contact_id,
            "companyContactRoleId": role_id,
            "companyLocationId": first_location_id
        }),
    ));
    assert_eq!(
        initial_assign.body["data"]["companyContactAssignRole"]["userErrors"],
        json!([])
    );

    let contact_bulk = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BBulkDuplicateContactAssignRoles(
          $companyContactId: ID!
          $rolesToAssign: [CompanyContactRoleAssign!]!
        ) {
          companyContactAssignRoles(
            companyContactId: $companyContactId
            rolesToAssign: $rolesToAssign
          ) {
            roleAssignments {
              id
              companyContact { id }
              role { id }
              companyLocation { id }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": first_contact_id,
            "rolesToAssign": [
                { "companyContactRoleId": role_id, "companyLocationId": first_location_id },
                { "companyContactRoleId": role_id, "companyLocationId": second_location_id }
            ]
        }),
    ));
    assert_eq!(contact_bulk.status, 200);
    let contact_assignments = contact_bulk.body["data"]["companyContactAssignRoles"]
        ["roleAssignments"]
        .as_array()
        .expect("contact bulk assignments");
    assert_eq!(contact_assignments.len(), 1);
    assert_eq!(
        contact_assignments[0]["companyContact"]["id"],
        json!(first_contact_id)
    );
    assert_eq!(
        contact_assignments[0]["companyLocation"]["id"],
        json!(second_location_id)
    );
    assert_eq!(
        contact_bulk.body["data"]["companyContactAssignRoles"]["userErrors"],
        json!([{
            "field": ["rolesToAssign", "0"],
            "message": "Company contact has already been assigned a role in that company location.",
            "code": "LIMIT_REACHED"
        }])
    );

    let location_bulk = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BBulkDuplicateLocationAssignRoles(
          $companyLocationId: ID!
          $rolesToAssign: [CompanyLocationRoleAssign!]!
        ) {
          companyLocationAssignRoles(
            companyLocationId: $companyLocationId
            rolesToAssign: $rolesToAssign
          ) {
            roleAssignments {
              id
              companyContact { id }
              role { id }
              companyLocation { id }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyLocationId": second_location_id,
            "rolesToAssign": [
                { "companyContactId": first_contact_id, "companyContactRoleId": role_id },
                { "companyContactId": second_contact_id, "companyContactRoleId": role_id }
            ]
        }),
    ));
    assert_eq!(location_bulk.status, 200);
    let location_assignments = location_bulk.body["data"]["companyLocationAssignRoles"]
        ["roleAssignments"]
        .as_array()
        .expect("location bulk assignments");
    assert_eq!(location_assignments.len(), 1);
    assert_eq!(
        location_assignments[0]["companyContact"]["id"],
        json!(second_contact_id)
    );
    assert_eq!(
        location_assignments[0]["companyLocation"]["id"],
        json!(second_location_id)
    );
    assert_eq!(
        location_bulk.body["data"]["companyLocationAssignRoles"]["userErrors"],
        json!([{
            "field": ["rolesToAssign", "0"],
            "message": "Company contact has already been assigned a role in that company location.",
            "code": "LIMIT_REACHED"
        }])
    );
}

#[test]
fn b2b_contact_revoke_role_singular_stages_and_reads_back() {
    let mut proxy = snapshot_proxy();
    let company = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactRevokeRoleCompany($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company {
              id
              contactRoles(first: 5) { nodes { id name } }
              locations(first: 5) { nodes { id } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Singular Revoke Co" } }),
    ));
    assert_eq!(company.status, 200);
    assert_eq!(
        company.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    let company_id = company.body["data"]["companyCreate"]["company"]["id"]
        .as_str()
        .expect("company id")
        .to_string();
    let role_id =
        company.body["data"]["companyCreate"]["company"]["contactRoles"]["nodes"][0]["id"].clone();
    let location_id =
        company.body["data"]["companyCreate"]["company"]["locations"]["nodes"][0]["id"].clone();
    let contact_id = create_b2b_company_contact(&mut proxy, &company_id, "Singular Buyer");

    let assign = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactRevokeRoleAssign(
          $companyContactId: ID!
          $companyContactRoleId: ID!
          $companyLocationId: ID!
        ) {
          companyContactAssignRole(
            companyContactId: $companyContactId
            companyContactRoleId: $companyContactRoleId
            companyLocationId: $companyLocationId
          ) {
            companyContactRoleAssignment { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": contact_id,
            "companyContactRoleId": role_id,
            "companyLocationId": location_id
        }),
    ));
    assert_eq!(assign.status, 200);
    assert_eq!(
        assign.body["data"]["companyContactAssignRole"]["userErrors"],
        json!([])
    );
    let assignment_id = assign.body["data"]["companyContactAssignRole"]
        ["companyContactRoleAssignment"]["id"]
        .clone();

    let revoke = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactRevokeRoleSingular(
          $companyContactId: ID!
          $companyContactRoleAssignmentId: ID!
        ) {
          companyContactRevokeRole(
            companyContactId: $companyContactId
            companyContactRoleAssignmentId: $companyContactRoleAssignmentId
          ) {
            revokedCompanyContactRoleAssignmentId
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": contact_id,
            "companyContactRoleAssignmentId": assignment_id
        }),
    ));
    assert_eq!(revoke.status, 200);
    assert_eq!(
        revoke.body["data"]["companyContactRevokeRole"]["revokedCompanyContactRoleAssignmentId"],
        assignment_id
    );
    assert_eq!(
        revoke.body["data"]["companyContactRevokeRole"]["userErrors"],
        json!([])
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BContactRevokeRoleRead($companyContactId: ID!, $companyLocationId: ID!) {
          companyContact(id: $companyContactId) {
            roleAssignments(first: 5) { nodes { id } }
          }
          companyLocation(id: $companyLocationId) {
            roleAssignments(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "companyContactId": contact_id, "companyLocationId": location_id }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["companyContact"]["roleAssignments"]["nodes"],
        json!([])
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["roleAssignments"]["nodes"],
        json!([])
    );
}

#[test]
fn b2b_company_location_lifecycle_stages_and_reads_back() {
    let mut proxy = snapshot_proxy();

    let create_company = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompanyWithDefaultLocation($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company {
              id
              name
              locations(first: 5) { nodes { id name shippingAddress { id address1 } billingAddress { id address1 } } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "company": { "name": "Acme B2B" },
                "companyLocation": {
                    "shippingAddress": { "address1": "123 Main", "city": "Boston", "countryCode": "US" },
                    "billingSameAsShipping": true
                }
            }
        }),
    ));
    assert_eq!(create_company.status, 200);
    assert_eq!(
        create_company.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    let company = &create_company.body["data"]["companyCreate"]["company"];
    let company_id = company["id"].as_str().expect("company id").to_string();
    let first_location = &company["locations"]["nodes"][0];
    assert_eq!(first_location["name"], json!("Acme B2B"));
    assert_eq!(
        first_location["shippingAddress"]["id"],
        first_location["billingAddress"]["id"]
    );

    let create_location = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationCreateFallback($companyId: ID!) {
          companyLocationCreate(
            companyId: $companyId,
            input: {
              phone: "+14155550100",
              shippingAddress: { address1: "456 Side", city: "Austin", countryCode: "US" }
            }
          ) {
            companyLocation { id name company { id name } shippingAddress { id address1 } billingAddress { id address1 } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id }),
    ));
    assert_eq!(create_location.status, 200);
    assert_eq!(
        create_location.body["data"]["companyLocationCreate"]["userErrors"],
        json!([])
    );
    let second_location_id = create_location.body["data"]["companyLocationCreate"]
        ["companyLocation"]["id"]
        .as_str()
        .expect("location id")
        .to_string();
    let second_shipping_id = create_location.body["data"]["companyLocationCreate"]
        ["companyLocation"]["shippingAddress"]["id"]
        .as_str()
        .expect("shipping address id")
        .to_string();
    assert_eq!(
        create_location.body["data"]["companyLocationCreate"]["companyLocation"],
        json!({
            "id": second_location_id,
            "name": "456 Side",
            "company": { "id": company_id, "name": "Acme B2B" },
            "shippingAddress": {
                "id": second_shipping_id,
                "address1": "456 Side"
            },
            "billingAddress": Value::Null
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BLocationLifecycleRead($companyId: ID!, $locationId: ID!) {
          company(id: $companyId) {
            id
            locations(first: 5) { nodes { id name } }
          }
          companyLocation(id: $locationId) {
            id
            name
            shippingAddress { id address1 }
            roleAssignments(first: 5) { nodes { id } }
            staffMemberAssignments(first: 5) { nodes { id } }
          }
          companyLocations(first: 5) { nodes { id name } }
        }
        "#,
        json!({ "companyId": company_id, "locationId": second_location_id }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["company"]["locations"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        read.body["data"]["company"]["locations"]["nodes"][0]["name"],
        json!("Acme B2B")
    );
    assert_eq!(
        read.body["data"]["company"]["locations"]["nodes"][1]["name"],
        json!("456 Side")
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["shippingAddress"]["address1"],
        json!("456 Side")
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["roleAssignments"]["nodes"],
        json!([])
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["staffMemberAssignments"]["nodes"],
        json!([])
    );
    assert_eq!(
        read.body["data"]["companyLocations"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("log entries");
    assert!(entries.iter().any(|entry| {
        entry["status"] == json!("staged")
            && entry["interpreted"]["primaryRootField"] == json!("companyLocationCreate")
            && entry["stagedResourceIds"]
                .as_array()
                .is_some_and(|ids| ids.iter().any(|id| id == &json!(second_location_id)))
    }));
}

#[test]
fn b2b_company_and_location_aggregate_fields_project_from_staged_orders() {
    let clock = Arc::new(Mutex::new(utc_time(1704067200)));
    let mut proxy = snapshot_proxy_with_clock(clock.clone());
    restore_shop_currency(&mut proxy, "USD");

    let create_company = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAggregatesCompanyCreate($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company {
              id
              totalSpent { amount currencyCode }
              spend: totalSpent { value: amount currencyCode }
              ordersCount { count precision }
              orderSummary: ordersCount { total: count precision }
              lifetimeDuration
              locations(first: 5) {
                nodes {
                  id
                  totalSpent { amount currencyCode }
                  currency
                  ordersCount { count precision }
                  orderCount
                  catalogs(first: 5) { nodes { id title } }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "company": { "name": "Aggregate Buyer" },
                "companyLocation": { "name": "Aggregate HQ" }
            }
        }),
    ));
    assert_eq!(create_company.status, 200);
    assert_eq!(
        create_company.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    let company = &create_company.body["data"]["companyCreate"]["company"];
    let company_id = company["id"].as_str().expect("company id").to_string();
    let location_id = company["locations"]["nodes"][0]["id"]
        .as_str()
        .expect("location id")
        .to_string();
    assert_eq!(
        company["totalSpent"],
        json!({ "amount": "0.0", "currencyCode": "USD" })
    );
    assert_eq!(
        company["spend"],
        json!({ "value": "0.0", "currencyCode": "USD" })
    );
    assert_eq!(
        company["ordersCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(
        company["orderSummary"],
        json!({ "total": 0, "precision": "EXACT" })
    );
    assert_eq!(company["lifetimeDuration"], json!("less than 5 seconds"));
    assert_eq!(
        company["locations"]["nodes"][0]["totalSpent"],
        json!({ "amount": "0.0", "currencyCode": "USD" })
    );
    assert_eq!(company["locations"]["nodes"][0]["currency"], json!("USD"));
    assert_eq!(
        company["locations"]["nodes"][0]["ordersCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(company["locations"]["nodes"][0]["orderCount"], json!(0));
    assert_eq!(
        company["locations"]["nodes"][0]["catalogs"]["nodes"],
        json!([])
    );

    set_clock(&clock, 1704067245);
    let create_order = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAggregatesOrderCreate($order: OrderCreateOrderInput!) {
          orderCreate(order: $order) {
            order { id currentTotalPriceSet { shopMoney { amount currencyCode } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "order": {
                "email": "aggregate-buyer@example.test",
                "currency": "USD",
                "financialStatus": "PENDING",
                "companyLocationId": location_id,
                "lineItems": [{
                    "title": "Aggregate item",
                    "quantity": 2,
                    "priceSet": { "shopMoney": { "amount": "12.50", "currencyCode": "USD" } }
                }]
            }
        }),
    ));
    assert_eq!(create_order.status, 200);
    assert_eq!(
        create_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        create_order.body["data"]["orderCreate"]["order"]["currentTotalPriceSet"]["shopMoney"],
        json!({ "amount": "25.0", "currencyCode": "USD" })
    );

    let create_catalog = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAggregatesCatalogCreate($input: CatalogCreateInput!) {
          catalogCreate(input: $input) {
            catalog { id title }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "title": "Aggregate Catalog",
                "status": "ACTIVE",
                "context": { "companyLocationIds": [location_id] }
            }
        }),
    ));
    assert_eq!(create_catalog.status, 200);
    assert_eq!(
        create_catalog.body["data"]["catalogCreate"]["userErrors"],
        json!([])
    );
    let catalog_id = create_catalog.body["data"]["catalogCreate"]["catalog"]["id"]
        .as_str()
        .expect("catalog id")
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BAggregatesRead($companyId: ID!, $locationId: ID!) {
          company(id: $companyId) {
            id
            totalSpent { amount currencyCode }
            spend: totalSpent { value: amount currencyCode }
            ordersCount { count precision }
            orderSummary: ordersCount { total: count precision }
            lifetimeDuration
          }
          companyLocation(id: $locationId) {
            id
            totalSpent { amount currencyCode }
            locationSpend: totalSpent { value: amount currencyCode }
            currency
            ordersCount { count precision }
            orderSummary: ordersCount { total: count precision }
            orderCount
            catalogs(first: 5) { nodes { id title } }
          }
          companyNode: node(id: $companyId) {
            __typename
            ... on Company {
              totalSpent { amount currencyCode }
              ordersCount { count precision }
              lifetimeDuration
            }
          }
          locationNode: node(id: $locationId) {
            __typename
            ... on CompanyLocation {
              totalSpent { amount currencyCode }
              currency
              ordersCount { count precision }
              orderCount
              catalogs(first: 5) { nodes { id title } }
            }
          }
        }
        "#,
        json!({ "companyId": company_id, "locationId": location_id }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["company"]["totalSpent"],
        json!({ "amount": "25.0", "currencyCode": "USD" })
    );
    assert_eq!(
        read.body["data"]["company"]["spend"],
        json!({ "value": "25.0", "currencyCode": "USD" })
    );
    assert_eq!(
        read.body["data"]["company"]["ordersCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["company"]["orderSummary"],
        json!({ "total": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["company"]["lifetimeDuration"],
        json!("less than a minute")
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["totalSpent"],
        json!({ "amount": "25.0", "currencyCode": "USD" })
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["locationSpend"],
        json!({ "value": "25.0", "currencyCode": "USD" })
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["currency"],
        json!("USD")
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["ordersCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["orderSummary"],
        json!({ "total": 1, "precision": "EXACT" })
    );
    assert_eq!(read.body["data"]["companyLocation"]["orderCount"], json!(1));
    assert_eq!(
        read.body["data"]["companyLocation"]["catalogs"]["nodes"],
        json!([{ "id": catalog_id, "title": "Aggregate Catalog" }])
    );
    let company_node = &read.body["data"]["companyNode"];
    let location_node = &read.body["data"]["locationNode"];
    assert_eq!(company_node["__typename"], json!("Company"));
    assert_eq!(
        company_node["totalSpent"],
        json!({ "amount": "25.0", "currencyCode": "USD" })
    );
    assert_eq!(
        company_node["ordersCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        company_node["lifetimeDuration"],
        json!("less than a minute")
    );
    assert_eq!(location_node["__typename"], json!("CompanyLocation"));
    assert_eq!(
        location_node["totalSpent"],
        json!({ "amount": "25.0", "currencyCode": "USD" })
    );
    assert_eq!(location_node["currency"], json!("USD"));
    assert_eq!(
        location_node["ordersCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(location_node["orderCount"], json!(1));
    assert_eq!(
        location_node["catalogs"]["nodes"],
        json!([{ "id": catalog_id, "title": "Aggregate Catalog" }])
    );

    set_clock(&clock, 1711929600);
    let create_with_customer_since = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLifetimeDurationCustomerSince($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company {
              id
              createdAt
              customerSince
              lifetimeDuration
              ordersCount { count precision }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "company": {
                    "name": "Customer Since Duration Co",
                    "customerSince": "2024-01-01T00:00:00Z"
                }
            }
        }),
    ));
    assert_eq!(create_with_customer_since.status, 200);
    assert_eq!(
        create_with_customer_since.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    let customer_since_company =
        &create_with_customer_since.body["data"]["companyCreate"]["company"];
    assert_eq!(
        customer_since_company["createdAt"],
        json!("2024-04-01T00:00:00Z")
    );
    assert_eq!(
        customer_since_company["customerSince"],
        json!("2024-01-01T00:00:00Z")
    );
    assert_eq!(
        customer_since_company["ordersCount"],
        json!({ "count": 0, "precision": "EXACT" })
    );
    assert_eq!(
        customer_since_company["lifetimeDuration"],
        json!("3 months")
    );
}

#[test]
fn b2b_company_location_aggregate_currency_uses_location_country_for_draft_orders() {
    let mut proxy = snapshot_proxy();

    let create_company = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAggregatesCanadianCompanyCreate($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company {
              id
              mainContact { id }
              locations(first: 1) {
                nodes {
                  id
                  currency
                  totalSpent { amount currencyCode }
                }
              }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "company": { "name": "Canadian Aggregate Buyer" },
                "companyContact": {
                    "firstName": "Canadian",
                    "lastName": "Buyer",
                    "email": "canadian-aggregate-buyer@example.test"
                },
                "companyLocation": {
                    "name": "Canadian Aggregate HQ",
                    "phone": "+16135550145",
                    "shippingAddress": {
                        "address1": "145 Aggregate Way",
                        "city": "Ottawa",
                        "countryCode": "CA"
                    }
                }
            }
        }),
    ));
    assert_eq!(create_company.status, 200);
    assert_eq!(
        create_company.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    let company = &create_company.body["data"]["companyCreate"]["company"];
    let company_id = company["id"].as_str().expect("company id").to_string();
    let contact_id = company["mainContact"]["id"]
        .as_str()
        .expect("main contact id")
        .to_string();
    let location = &company["locations"]["nodes"][0];
    let location_id = location["id"].as_str().expect("location id").to_string();
    assert_eq!(location["currency"], json!("CAD"));
    assert_eq!(
        location["totalSpent"],
        json!({ "amount": "0.0", "currencyCode": "CAD" })
    );

    let create_draft_order = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAggregatesDraftOrderCreate($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder {
              id
              totalPriceSet { shopMoney { amount currencyCode } }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "purchasingEntity": {
                    "purchasingCompany": {
                        "companyId": company_id,
                        "companyContactId": contact_id,
                        "companyLocationId": location_id
                    }
                },
                "email": "canadian-aggregate-draft@example.test",
                "lineItems": [{
                    "title": "Canadian aggregate item",
                    "quantity": 1,
                    "originalUnitPrice": "25.00",
                    "requiresShipping": false,
                    "taxable": false
                }]
            }
        }),
    ));
    assert_eq!(create_draft_order.status, 200);
    assert_eq!(
        create_draft_order.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let draft_order_id = create_draft_order.body["data"]["draftOrderCreate"]["draftOrder"]["id"]
        .as_str()
        .expect("draft order id")
        .to_string();
    assert_eq!(
        create_draft_order.body["data"]["draftOrderCreate"]["draftOrder"]["totalPriceSet"]
            ["shopMoney"],
        json!({ "amount": "25.0", "currencyCode": "CAD" })
    );

    let complete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAggregatesDraftOrderComplete($id: ID!) {
          draftOrderComplete(id: $id, paymentPending: false) {
            draftOrder {
              order {
                id
                currentTotalPriceSet { shopMoney { amount currencyCode } }
                purchasingEntity {
                  __typename
                  ... on PurchasingCompany {
                    company { id }
                    contact { id }
                    location { id }
                  }
                }
              }
            }
            userErrors { field message }
          }
        }
        "#,
        json!({ "id": draft_order_id }),
    ));
    assert_eq!(complete.status, 200);
    assert_eq!(
        complete.body["data"]["draftOrderComplete"]["userErrors"],
        json!([])
    );
    assert_eq!(
        complete.body["data"]["draftOrderComplete"]["draftOrder"]["order"]["currentTotalPriceSet"]
            ["shopMoney"],
        json!({ "amount": "25.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        complete.body["data"]["draftOrderComplete"]["draftOrder"]["order"]["purchasingEntity"],
        json!({
            "__typename": "PurchasingCompany",
            "company": { "id": company_id },
            "contact": { "id": contact_id },
            "location": { "id": location_id }
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BAggregatesCanadianRead($companyId: ID!, $locationId: ID!) {
          company(id: $companyId) {
            totalSpent { amount currencyCode }
            ordersCount { count precision }
          }
          companyLocation(id: $locationId) {
            currency
            totalSpent { amount currencyCode }
            ordersCount { count precision }
          }
        }
        "#,
        json!({ "companyId": company_id, "locationId": location_id }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["company"]["totalSpent"],
        json!({ "amount": "25.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        read.body["data"]["company"]["ordersCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["currency"],
        json!("CAD")
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["totalSpent"],
        json!({ "amount": "25.0", "currencyCode": "CAD" })
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["ordersCount"],
        json!({ "count": 1, "precision": "EXACT" })
    );
}

#[test]
fn b2b_company_and_location_order_sub_connections_are_state_backed() {
    let mut proxy = snapshot_proxy();
    restore_shop_currency(&mut proxy, "USD");

    let create_company = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BOrderConnectionsCompanyCreate($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company {
              id
              mainContact { id }
              locations(first: 1) { nodes { id } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "input": {
                "company": { "name": "Connection Buyer" },
                "companyContact": {
                    "firstName": "Connection",
                    "lastName": "Buyer",
                    "email": "connection-buyer@example.test"
                },
                "companyLocation": { "name": "Connection HQ" }
            }
        }),
    ));
    assert_eq!(create_company.status, 200);
    assert_eq!(
        create_company.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    let company = &create_company.body["data"]["companyCreate"]["company"];
    let company_id = company["id"].as_str().expect("company id").to_string();
    let contact_id = company["mainContact"]["id"]
        .as_str()
        .expect("contact id")
        .to_string();
    let location_id = company["locations"]["nodes"][0]["id"]
        .as_str()
        .expect("location id")
        .to_string();

    let empty_company_id = create_b2b_company(&mut proxy, "Empty Connection Buyer");
    let empty_location_id = create_b2b_location(&mut proxy, &empty_company_id, "Empty HQ");

    let create_order = |proxy: &mut DraftProxy, title: &str, amount: &str| {
        proxy.process_request(json_graphql_request(
            r#"
            mutation B2BOrderConnectionsOrderCreate($order: OrderCreateOrderInput!) {
              orderCreate(order: $order) {
                order { id name currentTotalPriceSet { shopMoney { amount currencyCode } } }
                userErrors { field message code }
              }
            }
            "#,
            json!({
                "order": {
                    "email": "connection-order@example.test",
                    "currency": "USD",
                    "companyLocationId": location_id,
                    "lineItems": [{
                        "title": title,
                        "quantity": 1,
                        "priceSet": { "shopMoney": { "amount": amount, "currencyCode": "USD" } }
                    }]
                }
            }),
        ))
    };
    let first_order = create_order(&mut proxy, "Connection first order", "12.50");
    let second_order = create_order(&mut proxy, "Connection second order", "7.50");
    assert_eq!(
        first_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        second_order.body["data"]["orderCreate"]["userErrors"],
        json!([])
    );
    let first_order_id = first_order.body["data"]["orderCreate"]["order"]["id"]
        .as_str()
        .expect("first order id")
        .to_string();
    let first_order_name = first_order.body["data"]["orderCreate"]["order"]["name"]
        .as_str()
        .expect("first order name")
        .to_string();
    let second_order_id = second_order.body["data"]["orderCreate"]["order"]["id"]
        .as_str()
        .expect("second order id")
        .to_string();
    let second_order_name = second_order.body["data"]["orderCreate"]["order"]["name"]
        .as_str()
        .expect("second order name")
        .to_string();

    let create_draft_order = |proxy: &mut DraftProxy, email: &str, title: &str| {
        proxy.process_request(json_graphql_request(
            r#"
            mutation B2BOrderConnectionsDraftOrderCreate($input: DraftOrderInput!) {
              draftOrderCreate(input: $input) {
                draftOrder { id name status totalPriceSet { shopMoney { amount currencyCode } } }
                userErrors { field message }
              }
            }
            "#,
            json!({
                "input": {
                    "presentmentCurrencyCode": "USD",
                    "purchasingEntity": {
                        "purchasingCompany": {
                            "companyId": company_id,
                            "companyContactId": contact_id,
                            "companyLocationId": location_id
                        }
                    },
                    "email": email,
                    "lineItems": [{
                        "title": title,
                        "quantity": 1,
                        "originalUnitPrice": "5.00",
                        "requiresShipping": false,
                        "taxable": false
                    }]
                }
            }),
        ))
    };
    let first_draft = create_draft_order(
        &mut proxy,
        "connection-draft-one@example.test",
        "Connection first draft",
    );
    let second_draft = create_draft_order(
        &mut proxy,
        "connection-draft-two@example.test",
        "Connection second draft",
    );
    assert_eq!(
        first_draft.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    assert_eq!(
        second_draft.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let first_draft_id = first_draft.body["data"]["draftOrderCreate"]["draftOrder"]["id"]
        .as_str()
        .expect("first draft id")
        .to_string();
    let first_draft_name = first_draft.body["data"]["draftOrderCreate"]["draftOrder"]["name"]
        .as_str()
        .expect("first draft name")
        .to_string();
    let first_draft_total =
        first_draft.body["data"]["draftOrderCreate"]["draftOrder"]["totalPriceSet"].clone();
    let second_draft_id = second_draft.body["data"]["draftOrderCreate"]["draftOrder"]["id"]
        .as_str()
        .expect("second draft id")
        .to_string();
    let second_draft_name = second_draft.body["data"]["draftOrderCreate"]["draftOrder"]["name"]
        .as_str()
        .expect("second draft name")
        .to_string();

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BOrderSubConnections(
          $companyId: ID!
          $locationId: ID!
          $emptyCompanyId: ID!
          $emptyLocationId: ID!
          $firstOrderCursor: String!
          $secondOrderCursor: String!
          $firstDraftCursor: String!
          $secondDraftCursor: String!
        ) {
          company(id: $companyId) {
            ordersCount { count precision }
            totalSpent { amount currencyCode }
            ordersFirst: orders(first: 1) {
              nodes { id name currentTotalPriceSet { shopMoney { amount currencyCode } } }
              edges { cursor node { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            ordersAfter: orders(first: 1, after: $firstOrderCursor) {
              nodes { id name }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            ordersBefore: orders(last: 1, before: $secondOrderCursor) {
              nodes { id name }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            draftOrdersFirst: draftOrders(first: 1) {
              nodes { id name status totalPriceSet { shopMoney { amount currencyCode } } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            draftOrdersAfter: draftOrders(first: 1, after: $firstDraftCursor) {
              nodes { id name }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          companyLocation(id: $locationId) {
            orderCount
            ordersAfter: orders(first: 1, after: $firstOrderCursor) {
              nodes { id name }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            draftOrdersBefore: draftOrders(last: 1, before: $secondDraftCursor) {
              nodes { id name }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          emptyCompany: company(id: $emptyCompanyId) {
            orders(first: 2) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            draftOrders(first: 2) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          emptyLocation: companyLocation(id: $emptyLocationId) {
            orders(first: 2) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            draftOrders(first: 2) {
              nodes { id }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({
            "companyId": company_id,
            "locationId": location_id,
            "emptyCompanyId": empty_company_id,
            "emptyLocationId": empty_location_id,
            "firstOrderCursor": first_order_id,
            "secondOrderCursor": second_order_id,
            "firstDraftCursor": first_draft_id,
            "secondDraftCursor": second_draft_id
        }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["company"]["ordersCount"],
        json!({ "count": 2, "precision": "EXACT" })
    );
    assert_eq!(
        read.body["data"]["company"]["totalSpent"],
        json!({ "amount": "20.0", "currencyCode": "USD" })
    );
    assert_eq!(
        read.body["data"]["company"]["ordersFirst"]["nodes"],
        json!([{
            "id": first_order_id,
            "name": first_order_name,
            "currentTotalPriceSet": {
                "shopMoney": { "amount": "12.5", "currencyCode": "USD" }
            }
        }])
    );
    assert_eq!(
        read.body["data"]["company"]["ordersFirst"]["edges"],
        json!([{ "cursor": first_order_id, "node": { "id": first_order_id } }])
    );
    assert_eq!(
        read.body["data"]["company"]["ordersFirst"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": first_order_id,
            "endCursor": first_order_id
        })
    );
    assert_eq!(
        read.body["data"]["company"]["ordersAfter"]["nodes"],
        json!([{ "id": second_order_id, "name": second_order_name }])
    );
    assert_eq!(
        read.body["data"]["company"]["ordersAfter"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": second_order_id,
            "endCursor": second_order_id
        })
    );
    assert_eq!(
        read.body["data"]["company"]["ordersBefore"]["nodes"],
        json!([{ "id": first_order_id, "name": first_order_name }])
    );
    assert_eq!(
        read.body["data"]["company"]["ordersBefore"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": first_order_id,
            "endCursor": first_order_id
        })
    );
    assert_eq!(
        read.body["data"]["company"]["draftOrdersFirst"]["nodes"],
        json!([{
            "id": first_draft_id,
            "name": first_draft_name,
            "status": "OPEN",
            "totalPriceSet": first_draft_total
        }])
    );
    assert_eq!(
        read.body["data"]["company"]["draftOrdersFirst"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": first_draft_id,
            "endCursor": first_draft_id
        })
    );
    assert_eq!(
        read.body["data"]["company"]["draftOrdersAfter"]["nodes"],
        json!([{ "id": second_draft_id, "name": second_draft_name }])
    );
    assert_eq!(read.body["data"]["companyLocation"]["orderCount"], json!(2));
    assert_eq!(
        read.body["data"]["companyLocation"]["ordersAfter"]["nodes"],
        json!([{ "id": second_order_id, "name": second_order_name }])
    );
    assert_eq!(
        read.body["data"]["companyLocation"]["draftOrdersBefore"]["nodes"],
        json!([{ "id": first_draft_id, "name": first_draft_name }])
    );
    let empty_page_info = json!({
        "hasNextPage": false,
        "hasPreviousPage": false,
        "startCursor": Value::Null,
        "endCursor": Value::Null
    });
    assert_eq!(
        read.body["data"]["emptyCompany"]["orders"]["nodes"],
        json!([])
    );
    assert_eq!(
        read.body["data"]["emptyCompany"]["orders"]["pageInfo"],
        empty_page_info
    );
    assert_eq!(
        read.body["data"]["emptyCompany"]["draftOrders"]["nodes"],
        json!([])
    );
    assert_eq!(
        read.body["data"]["emptyCompany"]["draftOrders"]["pageInfo"],
        empty_page_info
    );
    assert_eq!(
        read.body["data"]["emptyLocation"]["orders"]["nodes"],
        json!([])
    );
    assert_eq!(
        read.body["data"]["emptyLocation"]["orders"]["pageInfo"],
        empty_page_info
    );
    assert_eq!(
        read.body["data"]["emptyLocation"]["draftOrders"]["nodes"],
        json!([])
    );
    assert_eq!(
        read.body["data"]["emptyLocation"]["draftOrders"]["pageInfo"],
        empty_page_info
    );
}

#[test]
fn b2b_company_connections_filter_sort_reverse_count_and_window() {
    let mut proxy = snapshot_proxy();
    let acme_company_id = create_b2b_company(&mut proxy, "Acme Supplies");
    let zeta_company_id = create_b2b_company(&mut proxy, "Zeta Supplies");
    let beta_company_id = create_b2b_company(&mut proxy, "Beta Goods");
    let acme_remote_location_id = create_b2b_location(&mut proxy, &acme_company_id, "Remote Hub");
    let acme_alpha_location_id = create_b2b_location(&mut proxy, &acme_company_id, "Alpha Hub");

    let companies = proxy.process_request(json_graphql_request(
        r#"
        query B2BCompanyConnectionFilters($query: String!) {
          companies(first: 1, query: $query, sortKey: NAME, reverse: true) {
            nodes { id name }
            edges { cursor node { id } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
          companiesCount { count precision }
        }
        "#,
        json!({ "query": "name:Supplies" }),
    ));
    assert_eq!(companies.status, 200);
    assert_eq!(
        companies.body["data"]["companies"]["nodes"],
        json!([{ "id": zeta_company_id, "name": "Zeta Supplies" }])
    );
    assert_eq!(
        companies.body["data"]["companies"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": zeta_company_id,
            "endCursor": zeta_company_id
        })
    );
    assert_eq!(
        companies.body["data"]["companiesCount"],
        json!({ "count": 3, "precision": "EXACT" })
    );

    let locations = proxy.process_request(json_graphql_request(
        r#"
        query B2BCompanyLocationConnectionFilters($after: String!) {
          companyLocations(first: 1, after: $after, query: "name:Hub", sortKey: NAME) {
            nodes { id name company { id name } }
            pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
          }
        }
        "#,
        json!({ "after": acme_alpha_location_id }),
    ));
    assert_eq!(locations.status, 200);
    assert_eq!(
        locations.body["data"]["companyLocations"]["nodes"],
        json!([{
            "id": acme_remote_location_id,
            "name": "Remote Hub",
            "company": { "id": acme_company_id, "name": "Acme Supplies" }
        }])
    );
    assert_eq!(
        locations.body["data"]["companyLocations"]["pageInfo"],
        json!({
            "hasNextPage": false,
            "hasPreviousPage": true,
            "startCursor": acme_remote_location_id,
            "endCursor": acme_remote_location_id
        })
    );

    let unsupported_filter = proxy.process_request(json_graphql_request(
        r#"
        query B2BCompanyUnsupportedFilter {
          companies(first: 5, query: "unsupported:Acme") {
            nodes { id }
            pageInfo { hasNextPage hasPreviousPage }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(unsupported_filter.status, 200);
    assert_eq!(
        unsupported_filter.body["data"]["companies"]["nodes"],
        json!([])
    );

    assert_ne!(beta_company_id, acme_company_id);
}

#[test]
fn b2b_company_nested_connections_sort_reverse_and_window() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company_with_contact_and_role(&mut proxy);
    let default_location_id = read_b2b_company_location_ids(&mut proxy, &company_id)[0].clone();
    let secondary_contact_id = create_b2b_company_contact(&mut proxy, &company_id, "Secondary");
    let remote_location_id = create_b2b_location(&mut proxy, &company_id, "Remote HQ");
    let (main_contact_id, role_id) = read_b2b_first_contact_and_role(&mut proxy, &company_id);
    let secondary_assignment_id = assign_b2b_contact_role(
        &mut proxy,
        &secondary_contact_id,
        &role_id,
        &remote_location_id,
    );
    let main_remote_assignment_id =
        assign_b2b_contact_role(&mut proxy, &main_contact_id, &role_id, &remote_location_id);

    let nested = proxy.process_request(json_graphql_request(
        r#"
        query B2BNestedConnectionArgs($companyId: ID!, $contactId: ID!, $locationId: ID!) {
          company(id: $companyId) {
            contacts(first: 1, sortKey: ID, reverse: true) {
              nodes { id title }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
            locations(first: 1, sortKey: ID, reverse: true) {
              nodes { id name }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          companyContact(id: $contactId) {
            roleAssignments(first: 1, sortKey: ID, reverse: true) {
              nodes { id companyLocation { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
          companyLocation(id: $locationId) {
            roleAssignments(first: 1, sortKey: ID, reverse: true) {
              nodes { id companyContact { id } }
              pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
            }
          }
        }
        "#,
        json!({
            "companyId": company_id,
            "contactId": main_contact_id,
            "locationId": remote_location_id
        }),
    ));
    assert_eq!(nested.status, 200);
    assert_eq!(
        nested.body["data"]["company"]["contacts"]["nodes"],
        json!([{ "id": secondary_contact_id, "title": "Secondary" }])
    );
    assert_eq!(
        nested.body["data"]["company"]["contacts"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": secondary_contact_id,
            "endCursor": secondary_contact_id
        })
    );
    assert_eq!(
        nested.body["data"]["company"]["locations"]["nodes"],
        json!([{ "id": remote_location_id, "name": "Remote HQ" }])
    );
    assert_eq!(
        nested.body["data"]["company"]["locations"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": remote_location_id,
            "endCursor": remote_location_id
        })
    );
    assert_eq!(
        nested.body["data"]["companyContact"]["roleAssignments"]["nodes"],
        json!([{
            "id": main_remote_assignment_id,
            "companyLocation": { "id": remote_location_id }
        }])
    );
    assert_eq!(
        nested.body["data"]["companyContact"]["roleAssignments"]["pageInfo"],
        json!({
            "hasNextPage": true,
            "hasPreviousPage": false,
            "startCursor": main_remote_assignment_id,
            "endCursor": main_remote_assignment_id
        })
    );
    assert_eq!(
        nested.body["data"]["companyLocation"]["roleAssignments"]["nodes"],
        json!([{
            "id": main_remote_assignment_id,
            "companyContact": { "id": main_contact_id }
        }])
    );

    assert_ne!(default_location_id, remote_location_id);
    assert_ne!(secondary_assignment_id, main_remote_assignment_id);
}

#[test]
fn b2b_company_location_addresses_accept_shared_country_catalog() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "International Address Co");

    let create_location = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCreateGbLocation($companyId: ID!) {
          companyLocationCreate(
            companyId: $companyId,
            input: {
              name: "London HQ"
              shippingAddress: {
                address1: "10 Downing Street"
                city: "London"
                countryCode: "GB"
                zoneCode: "LND"
                zip: "SW1A 2AA"
              }
            }
          ) {
            companyLocation {
              id
              currency
              shippingAddress { address1 city countryCode zip }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id }),
    ));
    assert_eq!(create_location.status, 200);
    assert_eq!(
        create_location.body["data"]["companyLocationCreate"]["userErrors"],
        json!([])
    );
    let location = &create_location.body["data"]["companyLocationCreate"]["companyLocation"];
    let location_id = location["id"].as_str().expect("location id").to_string();
    assert_eq!(location["currency"], json!("GBP"));
    assert_eq!(
        location["shippingAddress"],
        json!({
            "address1": "10 Downing Street",
            "city": "London",
            "countryCode": "GB",
            "zip": "SW1A 2AA"
        })
    );

    let assign_de_address = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignDeAddress($locationId: ID!) {
          companyLocationAssignAddress(
            locationId: $locationId,
            address: {
              address1: "Unter den Linden 1"
              city: "Berlin"
              countryCode: "DE"
              zoneCode: "BE"
              zip: "10117"
            },
            addressTypes: [BILLING]
          ) {
            addresses { address1 city countryCode zip }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(assign_de_address.status, 200);
    assert_eq!(
        assign_de_address.body["data"]["companyLocationAssignAddress"]["userErrors"],
        json!([])
    );
    assert_eq!(
        assign_de_address.body["data"]["companyLocationAssignAddress"]["addresses"],
        json!([{
            "address1": "Unter den Linden 1",
            "city": "Berlin",
            "countryCode": "DE",
            "zip": "10117"
        }])
    );

    let invalid_country = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignInvalidCountry($locationId: ID!) {
          companyLocationAssignAddress(
            locationId: $locationId,
            address: { address1: "Unknown", countryCode: ZZ },
            addressTypes: [SHIPPING]
          ) {
            addresses { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(invalid_country.status, 200);
    assert_eq!(
        invalid_country.body["data"]["companyLocationAssignAddress"],
        json!({
            "addresses": Value::Null,
            "userErrors": [{
                "field": ["address", "countryCode"],
                "message": "Country code is invalid",
                "code": "INVALID"
            }]
        })
    );
}

#[test]
fn b2b_company_location_create_address_only_returns_no_input_without_staging() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "Address Only Co");

    let create_location = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationCreateAddressOnly($companyId: ID!) {
          companyLocationCreate(
            companyId: $companyId,
            input: { shippingAddress: { address1: "Only Address", countryCode: "US" } }
          ) {
            companyLocation { id name }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id }),
    ));
    assert_eq!(create_location.status, 200);
    assert_eq!(
        create_location.body["data"]["companyLocationCreate"],
        json!({
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": Value::Null,
                "message": "Company location create input is empty.",
                "code": "NO_INPUT"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BLocationCreateAddressOnlyRead($companyId: ID!) {
          company(id: $companyId) {
            locations(first: 5) { nodes { id name } }
          }
          companyLocations(first: 5) { nodes { id name } }
        }
        "#,
        json!({ "companyId": company_id }),
    ));
    assert_eq!(read.status, 200);
    assert_eq!(
        read.body["data"]["company"]["locations"]["nodes"]
            .as_array()
            .expect("company locations")
            .len(),
        1
    );
    assert_eq!(
        read.body["data"]["companyLocations"]["nodes"]
            .as_array()
            .expect("all locations")
            .len(),
        1
    );

    let log = log_snapshot(&proxy);
    let entries = log["entries"].as_array().expect("log entries");
    assert!(entries.iter().any(|entry| {
        entry["status"] == json!("failed")
            && entry["interpreted"]["primaryRootField"] == json!("companyLocationCreate")
            && entry["stagedResourceIds"]
                .as_array()
                .is_some_and(|ids| ids.is_empty())
    }));
}

#[test]
fn b2b_location_address_assignment_preserves_ids_and_delete_cascades_shared_address() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "Address Co");
    let location_id = create_b2b_location(&mut proxy, &company_id, "Address HQ");

    let first_assign = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignAddress($locationId: ID!, $address: MailingAddressInput!, $types: [CompanyAddressType!]!) {
          companyLocationAssignAddress(locationId: $locationId, address: $address, addressTypes: $types) {
            addresses { id address1 city countryCode }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": location_id,
            "address": { "address1": "1 Billing", "city": "Dallas", "countryCode": "US" },
            "types": ["BILLING"]
        }),
    ));
    assert_eq!(first_assign.status, 200);
    assert_eq!(
        first_assign.body["data"]["companyLocationAssignAddress"]["userErrors"],
        json!([])
    );
    let billing_id = first_assign.body["data"]["companyLocationAssignAddress"]["addresses"][0]
        ["id"]
        .as_str()
        .expect("billing address id")
        .to_string();

    let update_assign = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignAddress($locationId: ID!, $address: MailingAddressInput!, $types: [CompanyAddressType!]!) {
          companyLocationAssignAddress(locationId: $locationId, address: $address, addressTypes: $types) {
            addresses { id address1 city countryCode }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": location_id,
            "address": { "address1": "2 Billing Updated", "city": "Dallas", "countryCode": "US" },
            "types": ["BILLING"]
        }),
    ));
    assert_eq!(
        update_assign.body["data"]["companyLocationAssignAddress"]["addresses"][0]["id"],
        json!(billing_id)
    );
    assert_eq!(
        update_assign.body["data"]["companyLocationAssignAddress"]["addresses"][0]["address1"],
        json!("2 Billing Updated")
    );

    let duplicate = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BDuplicateAddressType($locationId: ID!) {
          companyLocationAssignAddress(
            locationId: $locationId,
            address: { address1: "Duplicate", countryCode: US },
            addressTypes: [BILLING, BILLING]
          ) {
            addresses { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(duplicate.status, 200);
    assert_eq!(
        duplicate.body["data"]["companyLocationAssignAddress"],
        json!({
            "addresses": Value::Null,
            "userErrors": [{
                "field": Value::Null,
                "message": "Invalid input.",
                "code": "INVALID_INPUT"
            }]
        })
    );

    let shared_location_id = create_b2b_location_with_shared_address(&mut proxy, &company_id);
    let shared_read = proxy.process_request(json_graphql_request(
        r#"
        query B2BSharedAddressRead($id: ID!) {
          companyLocation(id: $id) {
            id
            shippingAddress { id address1 }
            billingAddress { id address1 }
          }
        }
        "#,
        json!({ "id": shared_location_id }),
    ));
    let shared_address_id = shared_read.body["data"]["companyLocation"]["shippingAddress"]["id"]
        .as_str()
        .expect("shared address id")
        .to_string();
    assert_eq!(
        shared_read.body["data"]["companyLocation"]["billingAddress"]["id"],
        json!(shared_address_id)
    );

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCompanyAddressDelete($addressId: ID!) {
          companyAddressDelete(addressId: $addressId) {
            deletedAddressId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "addressId": shared_address_id }),
    ));
    assert_eq!(
        delete.body["data"]["companyAddressDelete"],
        json!({ "deletedAddressId": shared_address_id, "userErrors": [] })
    );

    let read_after_delete = proxy.process_request(json_graphql_request(
        r#"
        query B2BSharedAddressReadAfterDelete($id: ID!) {
          companyLocation(id: $id) {
            shippingAddress { id }
            billingAddress { id }
          }
        }
        "#,
        json!({ "id": shared_location_id }),
    ));
    assert_eq!(
        read_after_delete.body["data"]["companyLocation"],
        json!({
            "shippingAddress": Value::Null,
            "billingAddress": Value::Null
        })
    );
}

#[test]
fn b2b_location_delete_bulk_delete_and_indexed_errors() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "Delete Co");
    let first_location_id = create_b2b_location(&mut proxy, &company_id, "First");
    let second_location_id = create_b2b_location(&mut proxy, &company_id, "Second");

    let single_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationDelete($id: ID!) {
          companyLocationDelete(companyLocationId: $id) {
            deletedCompanyLocationId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": first_location_id }),
    ));
    assert_eq!(
        single_delete.body["data"]["companyLocationDelete"],
        json!({
            "deletedCompanyLocationId": first_location_id,
            "userErrors": []
        })
    );

    let bulk = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationsDelete($ids: [ID!]!) {
          companyLocationsDelete(companyLocationIds: $ids) {
            deletedCompanyLocationIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": [second_location_id, "gid://shopify/CompanyLocation/999"] }),
    ));
    assert_eq!(
        bulk.body["data"]["companyLocationsDelete"],
        json!({
            "deletedCompanyLocationIds": [second_location_id],
            "userErrors": [{
                "field": ["companyLocationIds", "1"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BLocationsAfterDelete($companyId: ID!) {
          company(id: $companyId) { locations(first: 5) { nodes { id } } }
          companyLocations(first: 5) { nodes { id } }
        }
        "#,
        json!({ "companyId": company_id }),
    ));
    // companyCreate provisions a default location (verified against live Shopify
    // by the `b2b-company-create-lifecycle` parity scenario), which is never
    // deleted here — so both reads must show the two deleted locations gone while
    // the default location persists.
    let company_location_ids: Vec<&str> = read.body["data"]["company"]["locations"]["nodes"]
        .as_array()
        .expect("company locations nodes")
        .iter()
        .map(|node| node["id"].as_str().unwrap_or_default())
        .collect();
    assert!(!company_location_ids.contains(&first_location_id.as_str()));
    assert!(!company_location_ids.contains(&second_location_id.as_str()));
    assert_eq!(company_location_ids.len(), 1);

    let global_location_ids: Vec<&str> = read.body["data"]["companyLocations"]["nodes"]
        .as_array()
        .expect("global company locations nodes")
        .iter()
        .map(|node| node["id"].as_str().unwrap_or_default())
        .collect();
    assert!(!global_location_ids.contains(&first_location_id.as_str()));
    assert!(!global_location_ids.contains(&second_location_id.as_str()));
    assert_eq!(global_location_ids.len(), 1);
}

#[test]
fn b2b_location_delete_rejects_failed_deletable_check_blockers() {
    let mut proxy = snapshot_proxy();

    let only_location_company = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BOnlyLocationDeleteCompany($input: CompanyCreateInput!) {
          companyCreate(input: $input) {
            company {
              id
              locations(first: 5) { nodes { id name } }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "input": { "company": { "name": "Only Location Blocked" } } }),
    ));
    let only_company_id = only_location_company.body["data"]["companyCreate"]["company"]["id"]
        .as_str()
        .expect("only location company id")
        .to_string();
    let only_location_id = only_location_company.body["data"]["companyCreate"]["company"]
        ["locations"]["nodes"][0]["id"]
        .as_str()
        .expect("only location id")
        .to_string();

    let only_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BOnlyLocationDelete($id: ID!) {
          companyLocationDelete(companyLocationId: $id) {
            deletedCompanyLocationId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": only_location_id }),
    ));
    assert_eq!(
        only_delete.body["data"]["companyLocationDelete"],
        json!({
            "deletedCompanyLocationId": Value::Null,
            "userErrors": [{
                "field": ["companyLocationId"],
                "message": "Failed to delete the company location.",
                "code": "FAILED_TO_DELETE"
            }]
        })
    );

    let draft_company_id = create_b2b_company_with_contact_and_role(&mut proxy);
    let draft_location_id = create_b2b_location(&mut proxy, &draft_company_id, "Draft Blocked");
    let draft_company = proxy.process_request(json_graphql_request(
        r#"
        query B2BDraftLocationDeleteSetup($id: ID!) {
          company(id: $id) {
            contacts(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "id": draft_company_id }),
    ));
    let draft_contact_id = draft_company.body["data"]["company"]["contacts"]["nodes"][0]["id"]
        .as_str()
        .expect("draft contact id")
        .to_string();
    let draft_order = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BDraftLocationDeleteBlock($input: DraftOrderInput!) {
          draftOrderCreate(input: $input) {
            draftOrder { id status }
            userErrors { field message }
          }
        }
        "#,
        json!({
            "input": {
                "purchasingEntity": {
                    "purchasingCompany": {
                        "companyId": draft_company_id,
                        "companyContactId": draft_contact_id,
                        "companyLocationId": draft_location_id
                    }
                },
                "email": "b2b-location-draft-block@example.com",
                "lineItems": [{
                    "title": "Draft blocker",
                    "quantity": 1,
                    "originalUnitPrice": "1.00"
                }]
            }
        }),
    ));
    assert_eq!(
        draft_order.body["data"]["draftOrderCreate"]["userErrors"],
        json!([])
    );
    let draft_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BDraftLocationDelete($id: ID!) {
          companyLocationDelete(companyLocationId: $id) {
            deletedCompanyLocationId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": draft_location_id }),
    ));
    assert_eq!(
        draft_delete.body["data"]["companyLocationDelete"]["userErrors"][0]["code"],
        json!("FAILED_TO_DELETE")
    );

    let credit_company_id = create_b2b_company(&mut proxy, "Store Credit Blocked");
    let credit_location_id = create_b2b_location(&mut proxy, &credit_company_id, "Credit Blocked");
    let credit = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationStoreCredit($id: ID!) {
          storeCreditAccountCredit(id: $id, creditInput: { creditAmount: { amount: "5.00", currencyCode: USD } }) {
            storeCreditAccountTransaction { account { id balance { amount currencyCode } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": credit_location_id }),
    ));
    assert_eq!(
        credit.body["data"]["storeCreditAccountCredit"]["userErrors"],
        json!([])
    );
    let credit_delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCreditLocationDelete($id: ID!) {
          companyLocationDelete(companyLocationId: $id) {
            deletedCompanyLocationId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": credit_location_id }),
    ));
    assert_eq!(
        credit_delete.body["data"]["companyLocationDelete"]["userErrors"][0]["code"],
        json!("FAILED_TO_DELETE")
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BBlockedLocationsRead($onlyCompanyId: ID!, $onlyLocationId: ID!, $draftLocationId: ID!, $creditLocationId: ID!) {
          company(id: $onlyCompanyId) { locations(first: 5) { nodes { id } } }
          onlyLocation: companyLocation(id: $onlyLocationId) { id }
          draftLocation: companyLocation(id: $draftLocationId) { id }
          creditLocation: companyLocation(id: $creditLocationId) { id }
        }
        "#,
        json!({
            "onlyCompanyId": only_company_id,
            "onlyLocationId": only_location_id,
            "draftLocationId": draft_location_id,
            "creditLocationId": credit_location_id
        }),
    ));
    assert_eq!(
        read.body["data"]["company"]["locations"]["nodes"][0]["id"],
        json!(only_location_id)
    );
    assert_eq!(
        read.body["data"]["onlyLocation"]["id"],
        json!(only_location_id)
    );
    assert_eq!(
        read.body["data"]["draftLocation"]["id"],
        json!(draft_location_id)
    );
    assert_eq!(
        read.body["data"]["creditLocation"]["id"],
        json!(credit_location_id)
    );
}

#[test]
fn b2b_locations_delete_preserves_partial_success_for_blocked_locations() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "Bulk Location Delete");
    let blocked_location_id = create_b2b_location(&mut proxy, &company_id, "Blocked Credit");
    let deleted_location_id = create_b2b_location(&mut proxy, &company_id, "Deleted OK");
    let unknown_location_id = "gid://shopify/CompanyLocation/999999999999999";

    let credit = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BBulkLocationStoreCredit($id: ID!) {
          storeCreditAccountCredit(id: $id, creditInput: { creditAmount: { amount: "7.00", currencyCode: USD } }) {
            storeCreditAccountTransaction { account { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": blocked_location_id }),
    ));
    assert_eq!(
        credit.body["data"]["storeCreditAccountCredit"]["userErrors"],
        json!([])
    );

    let bulk = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationsDeletePartial($ids: [ID!]!) {
          companyLocationsDelete(companyLocationIds: $ids) {
            deletedCompanyLocationIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": [blocked_location_id, deleted_location_id, unknown_location_id] }),
    ));
    let blocked_location_tail = blocked_location_id
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .split('?')
        .next()
        .unwrap_or_default();
    assert_eq!(
        bulk.body["data"]["companyLocationsDelete"],
        json!({
            "deletedCompanyLocationIds": [deleted_location_id],
            "userErrors": [
                {
                    "field": ["companyLocationIds", "0"],
                    "message": format!("Failed to delete CompanyLocation {blocked_location_tail}: CompanyLocation has non-zero store credit balance"),
                    "code": "INTERNAL_ERROR"
                },
                {
                    "field": ["companyLocationIds", "2"],
                    "message": "Resource requested does not exist.",
                    "code": "RESOURCE_NOT_FOUND"
                }
            ]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BLocationsDeletePartialRead($blockedId: ID!, $deletedId: ID!) {
          blocked: companyLocation(id: $blockedId) { id }
          deleted: companyLocation(id: $deletedId) { id }
        }
        "#,
        json!({ "blockedId": blocked_location_id, "deletedId": deleted_location_id }),
    ));
    assert_eq!(
        read.body["data"]["blocked"]["id"],
        json!(blocked_location_id)
    );
    assert_eq!(read.body["data"]["deleted"], Value::Null);
}

#[test]
fn b2b_location_delete_success_cascades_role_assignments_and_addresses() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company_with_contact_and_role(&mut proxy);
    let location_id = create_b2b_location_with_shared_address(&mut proxy, &company_id);
    let company = proxy.process_request(json_graphql_request(
        r#"
        query B2BLocationDeleteCascadeSetup($id: ID!) {
          company(id: $id) {
            contacts(first: 5) { nodes { id } }
            contactRoles(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "id": company_id }),
    ));
    let contact_id = company.body["data"]["company"]["contacts"]["nodes"][0]["id"]
        .as_str()
        .expect("contact id")
        .to_string();
    let role_id = company.body["data"]["company"]["contactRoles"]["nodes"][0]["id"]
        .as_str()
        .expect("role id")
        .to_string();

    let assign = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationDeleteCascadeAssignRole($companyContactId: ID!, $companyContactRoleId: ID!, $companyLocationId: ID!) {
          companyContactAssignRole(
            companyContactId: $companyContactId
            companyContactRoleId: $companyContactRoleId
            companyLocationId: $companyLocationId
          ) {
            companyContactRoleAssignment { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": contact_id,
            "companyContactRoleId": role_id,
            "companyLocationId": location_id
        }),
    ));
    assert_eq!(
        assign.body["data"]["companyContactAssignRole"]["userErrors"],
        json!([])
    );
    let assignment_id = assign.body["data"]["companyContactAssignRole"]
        ["companyContactRoleAssignment"]["id"]
        .as_str()
        .expect("assignment id")
        .to_string();

    let delete = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationDeleteCascade($id: ID!) {
          companyLocationDelete(companyLocationId: $id) {
            deletedCompanyLocationId
            userErrors { field message code }
          }
        }
        "#,
        json!({ "id": location_id }),
    ));
    assert_eq!(
        delete.body["data"]["companyLocationDelete"],
        json!({
            "deletedCompanyLocationId": location_id,
            "userErrors": []
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BLocationDeleteCascadeRead($companyId: ID!, $contactId: ID!, $locationId: ID!) {
          company(id: $companyId) { locations(first: 5) { nodes { id } } }
          companyContact(id: $contactId) { roleAssignments(first: 5) { nodes { id } } }
          companyLocation(id: $locationId) { id shippingAddress { id } billingAddress { id } }
        }
        "#,
        json!({ "companyId": company_id, "contactId": contact_id, "locationId": location_id }),
    ));
    let company_location_ids = read.body["data"]["company"]["locations"]["nodes"]
        .as_array()
        .expect("company location nodes");
    assert!(!company_location_ids
        .iter()
        .any(|location| location["id"] == json!(location_id)));
    let contact_assignment_ids = read.body["data"]["companyContact"]["roleAssignments"]["nodes"]
        .as_array()
        .expect("contact assignment nodes");
    assert!(!contact_assignment_ids
        .iter()
        .any(|assignment| assignment["id"] == json!(assignment_id)));
    assert_eq!(read.body["data"]["companyLocation"], Value::Null);
}

#[test]
fn b2b_staff_assignment_rejects_unobserved_numeric_staff_member_ids() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "Unobserved Staff Co");
    let location_id = create_b2b_location(&mut proxy, &company_id, "Unobserved Staff HQ");

    let assign = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BRejectUnobservedStaff($locationId: ID!, $staff: [ID!]!) {
          companyLocationAssignStaffMembers(companyLocationId: $locationId, staffMemberIds: $staff) {
            companyLocationStaffMemberAssignments { id staffMember { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": location_id,
            "staff": ["gid://shopify/StaffMember/1"]
        }),
    ));
    assert_eq!(assign.status, 200);
    assert_eq!(
        assign.body["data"]["companyLocationAssignStaffMembers"],
        json!({
            "companyLocationStaffMemberAssignments": null,
            "userErrors": [{
                "field": ["staffMemberIds", "0"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );
}

#[test]
fn b2b_staff_remove_unknown_assignment_reports_indexed_error() {
    let mut proxy = snapshot_proxy();

    let remove = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BRemoveStaff($ids: [ID!]!) {
          companyLocationRemoveStaffMembers(companyLocationStaffMemberAssignmentIds: $ids) {
            deletedCompanyLocationStaffMemberAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": ["gid://shopify/CompanyLocationStaffMemberAssignment/999"] }),
    ));
    assert_eq!(
        remove.body["data"]["companyLocationRemoveStaffMembers"],
        json!({
            "deletedCompanyLocationStaffMemberAssignmentIds": null,
            "userErrors": [{
                "field": ["companyLocationStaffMemberAssignmentIds", "0"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );
}

#[test]
fn b2b_contact_revoke_role_validates_contact_before_assignment_scope() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company_with_contact_and_role(&mut proxy);
    let location_id = create_b2b_location(&mut proxy, &company_id, "Singular Scope HQ");
    let (contact_id, role_id) = read_b2b_first_contact_and_role(&mut proxy, &company_id);
    let assignment_id = assign_b2b_contact_role(&mut proxy, &contact_id, &role_id, &location_id);
    let missing_contact_id = "gid://shopify/CompanyContact/404?shopify-draft-proxy=synthetic";

    let revoke = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactRevokeRoleMissingContact(
          $companyContactId: ID!
          $companyContactRoleAssignmentId: ID!
        ) {
          companyContactRevokeRole(
            companyContactId: $companyContactId
            companyContactRoleAssignmentId: $companyContactRoleAssignmentId
          ) {
            revokedCompanyContactRoleAssignmentId
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": missing_contact_id,
            "companyContactRoleAssignmentId": assignment_id
        }),
    ));
    assert_eq!(
        revoke.body["data"]["companyContactRevokeRole"],
        json!({
            "revokedCompanyContactRoleAssignmentId": Value::Null,
            "userErrors": [{
                "field": ["companyContactId"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BContactRevokeRoleMissingContactRead($companyContactId: ID!) {
          companyContact(id: $companyContactId) {
            roleAssignments(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "companyContactId": contact_id }),
    ));
    let remaining_assignments = read.body["data"]["companyContact"]["roleAssignments"]["nodes"]
        .as_array()
        .expect("remaining assignments");
    assert!(remaining_assignments
        .iter()
        .any(|assignment| assignment["id"] == assignment_id));
}

#[test]
fn b2b_contact_revoke_roles_validates_empty_parent_and_assignment_scope() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company_with_contact_and_role(&mut proxy);
    let location_id = create_b2b_location(&mut proxy, &company_id, "Bulk Scope HQ");
    let (main_contact_id, role_id) = read_b2b_first_contact_and_role(&mut proxy, &company_id);
    let secondary_contact_id =
        create_b2b_company_contact(&mut proxy, &company_id, "Secondary Scope Buyer");
    let main_assignment_id =
        assign_b2b_contact_role(&mut proxy, &main_contact_id, &role_id, &location_id);
    let secondary_assignment_id =
        assign_b2b_contact_role(&mut proxy, &secondary_contact_id, &role_id, &location_id);

    let empty = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactRevokeRolesRequiresIds(
          $companyContactId: ID!
          $roleAssignmentIds: [ID!]!
          $revokeAll: Boolean
        ) {
          companyContactRevokeRoles(
            companyContactId: $companyContactId
            roleAssignmentIds: $roleAssignmentIds
            revokeAll: $revokeAll
          ) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": main_contact_id,
            "roleAssignmentIds": [],
            "revokeAll": false
        }),
    ));
    assert_eq!(
        empty.body["data"]["companyContactRevokeRoles"],
        json!({
            "revokedRoleAssignmentIds": Value::Null,
            "userErrors": [{
                "field": Value::Null,
                "message": "Invalid input.",
                "code": "INVALID_INPUT"
            }]
        })
    );

    let missing_contact_id = "gid://shopify/CompanyContact/404?shopify-draft-proxy=synthetic";
    let missing_contact = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactRevokeRolesMissingContact(
          $companyContactId: ID!
          $roleAssignmentIds: [ID!]!
        ) {
          companyContactRevokeRoles(
            companyContactId: $companyContactId
            roleAssignmentIds: $roleAssignmentIds
          ) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": missing_contact_id,
            "roleAssignmentIds": [main_assignment_id]
        }),
    ));
    assert_eq!(
        missing_contact.body["data"]["companyContactRevokeRoles"],
        json!({
            "revokedRoleAssignmentIds": Value::Null,
            "userErrors": [{
                "field": ["companyContactId"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let partial = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BContactRevokeRolesWrongContact(
          $companyContactId: ID!
          $roleAssignmentIds: [ID!]!
          $revokeAll: Boolean
        ) {
          companyContactRevokeRoles(
            companyContactId: $companyContactId
            roleAssignmentIds: $roleAssignmentIds
            revokeAll: $revokeAll
          ) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": main_contact_id,
            "roleAssignmentIds": [main_assignment_id, secondary_assignment_id],
            "revokeAll": false
        }),
    ));
    assert_eq!(
        partial.body["data"]["companyContactRevokeRoles"],
        json!({
            "revokedRoleAssignmentIds": [main_assignment_id],
            "userErrors": [{
                "field": ["roleAssignmentIds", "1"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BContactRevokeRolesWrongContactRead($companyContactId: ID!) {
          companyContact(id: $companyContactId) {
            roleAssignments(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "companyContactId": secondary_contact_id }),
    ));
    assert_eq!(
        read.body["data"]["companyContact"]["roleAssignments"]["nodes"],
        json!([{ "id": secondary_assignment_id }])
    );
}

#[test]
fn b2b_location_role_assign_revoke_validates_per_index() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company_with_contact_and_role(&mut proxy);
    let location_id = create_b2b_location(&mut proxy, &company_id, "Role HQ");
    let company = proxy.process_request(json_graphql_request(
        r#"
        query B2BCompanyRoleSetup($id: ID!) {
          company(id: $id) {
            contacts(first: 5) { nodes { id } }
            contactRoles(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "id": company_id }),
    ));
    let contact_id = company.body["data"]["company"]["contacts"]["nodes"][0]["id"]
        .as_str()
        .expect("contact id")
        .to_string();
    let role_id = company.body["data"]["company"]["contactRoles"]["nodes"][0]["id"]
        .as_str()
        .expect("role id")
        .to_string();

    let assign = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignRoles($locationId: ID!, $roles: [CompanyLocationRoleAssign!]!) {
          companyLocationAssignRoles(companyLocationId: $locationId, rolesToAssign: $roles) {
            roleAssignments {
              id
              companyContact { id }
              role { id }
              companyLocation { id }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": location_id,
            "roles": [
                { "companyContactId": contact_id, "companyContactRoleId": role_id },
                { "companyContactId": "gid://shopify/CompanyContact/999", "companyContactRoleId": role_id },
                { "companyContactId": contact_id, "companyContactRoleId": "gid://shopify/CompanyContactRole/999" }
            ]
        }),
    ));
    assert_eq!(assign.status, 200);
    assert_eq!(
        assign.body["data"]["companyLocationAssignRoles"]["roleAssignments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        assign.body["data"]["companyLocationAssignRoles"]["userErrors"],
        json!([
            {
                "field": ["rolesToAssign", "1"],
                "message": "Company contact does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            },
            {
                "field": ["rolesToAssign", "2"],
                "message": "Company role does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }
        ])
    );
    let assignment_id = assign.body["data"]["companyLocationAssignRoles"]["roleAssignments"][0]
        ["id"]
        .as_str()
        .expect("role assignment id")
        .to_string();

    let revoke = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BRevokeRoles($locationId: ID!, $roles: [ID!]!) {
          companyLocationRevokeRoles(companyLocationId: $locationId, rolesToRevoke: $roles) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": location_id,
            "roles": [assignment_id, "gid://shopify/CompanyContactRoleAssignment/999"]
        }),
    ));
    assert_eq!(
        revoke.body["data"]["companyLocationRevokeRoles"],
        json!({
            "revokedRoleAssignmentIds": [assignment_id],
            "userErrors": [{
                "field": ["rolesToRevoke", "1"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );
}

#[test]
fn b2b_location_revoke_roles_validates_parent_and_assignment_scope() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company_with_contact_and_role(&mut proxy);
    let first_location_id = create_b2b_location(&mut proxy, &company_id, "First Scope HQ");
    let second_location_id = create_b2b_location(&mut proxy, &company_id, "Second Scope HQ");
    let (contact_id, role_id) = read_b2b_first_contact_and_role(&mut proxy, &company_id);
    let first_assignment_id =
        assign_b2b_contact_role(&mut proxy, &contact_id, &role_id, &first_location_id);
    let second_assignment_id =
        assign_b2b_contact_role(&mut proxy, &contact_id, &role_id, &second_location_id);

    let missing_location_id = "gid://shopify/CompanyLocation/404?shopify-draft-proxy=synthetic";
    let missing_location = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationRevokeRolesMissingLocation($locationId: ID!, $roles: [ID!]!) {
          companyLocationRevokeRoles(companyLocationId: $locationId, rolesToRevoke: $roles) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": missing_location_id, "roles": [first_assignment_id] }),
    ));
    assert_eq!(
        missing_location.body["data"]["companyLocationRevokeRoles"],
        json!({
            "revokedRoleAssignmentIds": Value::Null,
            "userErrors": [{
                "field": ["companyLocationId"],
                "message": "Location does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let wrong_location = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BLocationRevokeRolesWrongLocation($locationId: ID!, $roles: [ID!]!) {
          companyLocationRevokeRoles(companyLocationId: $locationId, rolesToRevoke: $roles) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": first_location_id, "roles": [second_assignment_id] }),
    ));
    assert_eq!(
        wrong_location.body["data"]["companyLocationRevokeRoles"],
        json!({
            "revokedRoleAssignmentIds": [],
            "userErrors": [{
                "field": ["rolesToRevoke", "0"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
    );

    let read = proxy.process_request(json_graphql_request(
        r#"
        query B2BLocationRevokeRolesWrongLocationRead($companyLocationId: ID!) {
          companyLocation(id: $companyLocationId) {
            roleAssignments(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "companyLocationId": second_location_id }),
    ));
    assert_eq!(
        read.body["data"]["companyLocation"]["roleAssignments"]["nodes"],
        json!([{ "id": second_assignment_id }])
    );
}

#[test]
fn b2b_bulk_action_size_cap_rejects_oversized_inputs_before_validation() {
    assert_b2b_bulk_limit_response(
        r#"
        mutation B2BBulkLimitCompaniesDelete($ids: [ID!]!) {
          companiesDelete(companyIds: $ids) {
            deletedCompanyIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": b2b_test_ids("Company", 51) }),
        "companiesDelete",
        "companyIds",
        &["deletedCompanyIds"],
    );
    assert_b2b_bulk_limit_response(
        r#"
        mutation B2BBulkLimitContactsDelete($ids: [ID!]!) {
          companyContactsDelete(companyContactIds: $ids) {
            deletedCompanyContactIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": b2b_test_ids("CompanyContact", 51) }),
        "companyContactsDelete",
        "companyContactIds",
        &["deletedCompanyContactIds"],
    );
    assert_b2b_bulk_limit_response(
        r#"
        mutation B2BBulkLimitLocationsDelete($ids: [ID!]!) {
          companyLocationsDelete(companyLocationIds: $ids) {
            deletedCompanyLocationIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": b2b_test_ids("CompanyLocation", 51) }),
        "companyLocationsDelete",
        "companyLocationIds",
        &["deletedCompanyLocationIds"],
    );
    assert_b2b_bulk_limit_response(
        r#"
        mutation B2BBulkLimitContactAssignRoles(
          $companyContactId: ID!
          $roles: [CompanyContactRoleAssign!]!
        ) {
          companyContactAssignRoles(companyContactId: $companyContactId, rolesToAssign: $roles) {
            roleAssignments { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": "gid://shopify/CompanyContact/404",
            "roles": b2b_contact_role_assign_inputs(51)
        }),
        "companyContactAssignRoles",
        "rolesToAssign",
        &["roleAssignments"],
    );
    assert_b2b_bulk_limit_response(
        r#"
        mutation B2BBulkLimitLocationAssignRoles(
          $companyLocationId: ID!
          $roles: [CompanyLocationRoleAssign!]!
        ) {
          companyLocationAssignRoles(companyLocationId: $companyLocationId, rolesToAssign: $roles) {
            roleAssignments { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyLocationId": "gid://shopify/CompanyLocation/404",
            "roles": b2b_location_role_assign_inputs(51)
        }),
        "companyLocationAssignRoles",
        "rolesToAssign",
        &["roleAssignments"],
    );
    assert_b2b_bulk_limit_response(
        r#"
        mutation B2BBulkLimitContactRevokeRoles($companyContactId: ID!, $ids: [ID!]!) {
          companyContactRevokeRoles(companyContactId: $companyContactId, roleAssignmentIds: $ids) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": "gid://shopify/CompanyContact/404",
            "ids": b2b_test_ids("CompanyContactRoleAssignment", 51)
        }),
        "companyContactRevokeRoles",
        "roleAssignmentIds",
        &["revokedRoleAssignmentIds"],
    );
    assert_b2b_bulk_limit_response(
        r#"
        mutation B2BBulkLimitLocationRevokeRoles($companyLocationId: ID!, $ids: [ID!]!) {
          companyLocationRevokeRoles(companyLocationId: $companyLocationId, rolesToRevoke: $ids) {
            revokedRoleAssignmentIds
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyLocationId": "gid://shopify/CompanyLocation/404",
            "ids": b2b_test_ids("CompanyContactRoleAssignment", 51)
        }),
        "companyLocationRevokeRoles",
        "rolesToRevoke",
        &["revokedRoleAssignmentIds", "revokedRoleAssignmentIds"],
    );
    assert_b2b_bulk_limit_response(
        r#"
        mutation B2BBulkLimitAssignStaff($companyLocationId: ID!, $staff: [ID!]!) {
          companyLocationAssignStaffMembers(companyLocationId: $companyLocationId, staffMemberIds: $staff) {
            companyLocationStaffMemberAssignments { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyLocationId": "gid://shopify/CompanyLocation/404",
            "staff": b2b_test_ids("StaffMember", 51)
        }),
        "companyLocationAssignStaffMembers",
        "staffMemberIds",
        &["companyLocationStaffMemberAssignments"],
    );
    assert_b2b_bulk_limit_response(
        r#"
        mutation B2BBulkLimitRemoveStaff($ids: [ID!]!) {
          companyLocationRemoveStaffMembers(companyLocationStaffMemberAssignmentIds: $ids) {
            deletedCompanyLocationStaffMemberAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": b2b_test_ids("CompanyLocationStaffMemberAssignment", 51) }),
        "companyLocationRemoveStaffMembers",
        "companyLocationStaffMemberAssignmentIds",
        &["deletedCompanyLocationStaffMemberAssignmentIds"],
    );
}

#[test]
fn b2b_bulk_action_size_cap_rejects_before_live_hybrid_hydration() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None)
        .with_upstream_transport(|_| panic!("oversized local input must not hydrate upstream"));
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BBulkLimitCompaniesDeleteBeforeHydration($ids: [ID!]!) {
          companiesDelete(companyIds: $ids) {
            deletedCompanyIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": b2b_test_ids("Company", 51) }),
    ));

    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companiesDelete"],
        json!({
            "deletedCompanyIds": [],
            "userErrors": [b2b_bulk_limit_error("companyIds")]
        })
    );
}

#[test]
fn b2b_bulk_action_size_cap_keeps_oversized_valid_company_delete_atomic() {
    let mut proxy = snapshot_proxy();
    let company_ids = (0..51)
        .map(|index| create_b2b_company(&mut proxy, &format!("Bulk Limit {index}")))
        .collect::<Vec<_>>();
    let state_before = state_snapshot(&proxy);

    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BBulkLimitCompaniesDeleteValid($ids: [ID!]!) {
          companiesDelete(companyIds: $ids) {
            deletedCompanyIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": company_ids }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companiesDelete"],
        json!({
            "deletedCompanyIds": [],
            "userErrors": [b2b_bulk_limit_error("companyIds")]
        })
    );
    assert_eq!(state_snapshot(&proxy), state_before);
}

#[test]
fn b2b_bulk_action_size_cap_allows_fifty_entries_to_use_normal_validation() {
    let mut proxy = snapshot_proxy();
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BBulkLimitCompaniesDeleteBoundary($ids: [ID!]!) {
          companiesDelete(companyIds: $ids) {
            deletedCompanyIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": b2b_test_ids("Company", 50) }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companiesDelete"]["deletedCompanyIds"],
        json!([])
    );
    let errors = response.body["data"]["companiesDelete"]["userErrors"]
        .as_array()
        .expect("boundary user errors");
    assert_eq!(errors.len(), 50);
    assert_eq!(errors[0]["field"], json!(["companyIds", "0"]));
    assert_eq!(errors[49]["field"], json!(["companyIds", "49"]));
    assert!(errors
        .iter()
        .all(|error| error["code"] == json!("RESOURCE_NOT_FOUND")));
}

const B2B_HYDRATE_TEST_COMPANY_ID: &str = "gid://shopify/Company/991001";
const B2B_HYDRATE_TEST_LOCATION_ID: &str = "gid://shopify/CompanyLocation/992001";
const B2B_HYDRATE_TEST_SIBLING_LOCATION_ID: &str = "gid://shopify/CompanyLocation/992002";
const B2B_HYDRATE_TEST_CONTACT_ID: &str = "gid://shopify/CompanyContact/993001";
const B2B_HYDRATE_TEST_ROLE_ID: &str = "gid://shopify/CompanyContactRole/994001";
const B2B_HYDRATE_TEST_ROLE_ASSIGNMENT_ID: &str =
    "gid://shopify/CompanyContactRoleAssignment/995001";
const B2B_HYDRATE_TEST_STAFF_ID: &str = "gid://shopify/StaffMember/996001";
const B2B_HYDRATE_TEST_STAFF_ASSIGNMENT_ID: &str =
    "gid://shopify/CompanyLocationStaffMemberAssignment/997001";
const B2B_HYDRATE_TEST_CUSTOMER_ID: &str = "gid://shopify/Customer/998001";
const B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID: &str = "gid://shopify/Customer/998002";
const B2B_HYDRATE_TEST_ADDRESS_ID: &str = "gid://shopify/CompanyAddress/999001";

fn b2b_mutation_first_hydrate_test_response(body: &Value) -> Response {
    let data = match body["operationName"].as_str() {
        Some("B2BMutationTargetsHydrate") => {
            let nodes = body["variables"]["ids"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|id| {
                    id.as_str()
                        .map(b2b_mutation_first_hydrate_test_node)
                        .unwrap_or(Value::Null)
                })
                .collect::<Vec<_>>();
            json!({ "nodes": nodes })
        }
        Some("B2BMutationSearchHydrate") => json!({
            "companies": { "nodes": [] },
            "companyLocations": { "nodes": [] },
            "customers": { "nodes": [] }
        }),
        other => panic!("unexpected B2B hydrate test operation: {other:?}"),
    };
    Response {
        status: 200,
        headers: Default::default(),
        body: json!({ "data": data }),
    }
}

fn b2b_mutation_first_hydrate_test_node(id: &str) -> Value {
    match id {
        B2B_HYDRATE_TEST_COMPANY_ID => b2b_mutation_first_hydrate_test_company(),
        B2B_HYDRATE_TEST_LOCATION_ID | B2B_HYDRATE_TEST_SIBLING_LOCATION_ID => {
            let mut location = b2b_mutation_first_hydrate_test_location();
            location["id"] = json!(id);
            if id == B2B_HYDRATE_TEST_SIBLING_LOCATION_ID {
                location["name"] = json!("Hydrated sibling");
                location["billingAddress"] = Value::Null;
                location["roleAssignments"] = json!({ "nodes": [] });
                location["staffMemberAssignments"] = json!({ "nodes": [] });
            }
            location
        }
        B2B_HYDRATE_TEST_CONTACT_ID => b2b_mutation_first_hydrate_test_contact(),
        B2B_HYDRATE_TEST_ROLE_ID => json!({
            "__typename": "CompanyContactRole",
            "id": B2B_HYDRATE_TEST_ROLE_ID,
            "name": "Ordering only",
            "note": Value::Null
        }),
        B2B_HYDRATE_TEST_ROLE_ASSIGNMENT_ID => json!({
            "__typename": "CompanyContactRoleAssignment",
            "id": B2B_HYDRATE_TEST_ROLE_ASSIGNMENT_ID,
            "companyContact": b2b_mutation_first_hydrate_test_contact(),
            "companyLocation": b2b_mutation_first_hydrate_test_location(),
            "role": {
                "__typename": "CompanyContactRole",
                "id": B2B_HYDRATE_TEST_ROLE_ID,
                "name": "Ordering only",
                "note": Value::Null
            }
        }),
        B2B_HYDRATE_TEST_STAFF_ID => json!({
            "__typename": "StaffMember",
            "id": B2B_HYDRATE_TEST_STAFF_ID
        }),
        B2B_HYDRATE_TEST_STAFF_ASSIGNMENT_ID => json!({
            "__typename": "CompanyLocationStaffMemberAssignment",
            "id": B2B_HYDRATE_TEST_STAFF_ASSIGNMENT_ID,
            "companyLocation": b2b_mutation_first_hydrate_test_location(),
            "staffMember": { "id": B2B_HYDRATE_TEST_STAFF_ID }
        }),
        B2B_HYDRATE_TEST_CUSTOMER_ID | B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID => json!({
            "__typename": "Customer",
            "id": id,
            "firstName": "Hydrated",
            "lastName": "Customer",
            "displayName": "Hydrated Customer",
            "email": if id == B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID {
                "assignable@example.com"
            } else {
                "buyer@example.com"
            },
            "phone": Value::Null,
            "locale": "en",
            "defaultEmailAddress": {
                "emailAddress": if id == B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID {
                    "assignable@example.com"
                } else {
                    "buyer@example.com"
                }
            },
            "defaultPhoneNumber": Value::Null,
            "companyContactProfiles": if id == B2B_HYDRATE_TEST_ASSIGNABLE_CUSTOMER_ID {
                json!([])
            } else {
                json!([b2b_mutation_first_hydrate_test_contact()])
            }
        }),
        B2B_HYDRATE_TEST_ADDRESS_ID => json!({
            "__typename": "CompanyAddress",
            "id": B2B_HYDRATE_TEST_ADDRESS_ID,
            "address1": "1 Hydration Way",
            "city": "Toronto",
            "countryCode": "CA"
        }),
        _ => Value::Null,
    }
}

fn b2b_mutation_first_hydrate_test_company() -> Value {
    json!({
        "__typename": "Company",
        "id": B2B_HYDRATE_TEST_COMPANY_ID,
        "name": "Hydrated Buyer",
        "externalId": "HYDRATED-BUYER",
        "locations": {
            "nodes": [
                { "id": B2B_HYDRATE_TEST_LOCATION_ID, "name": "Hydrated HQ" },
                { "id": B2B_HYDRATE_TEST_SIBLING_LOCATION_ID, "name": "Hydrated sibling" }
            ],
            "pageInfo": { "hasNextPage": false, "endCursor": Value::Null }
        },
        "contacts": {
            "nodes": [{
                "id": B2B_HYDRATE_TEST_CONTACT_ID,
                "title": "Buyer",
                "isMainContact": true
            }],
            "pageInfo": { "hasNextPage": false, "endCursor": Value::Null }
        },
        "contactRoles": {
            "nodes": [{
                "__typename": "CompanyContactRole",
                "id": B2B_HYDRATE_TEST_ROLE_ID,
                "name": "Ordering only",
                "note": Value::Null
            }],
            "pageInfo": { "hasNextPage": false, "endCursor": Value::Null }
        },
        "mainContact": {
            "id": B2B_HYDRATE_TEST_CONTACT_ID,
            "title": "Buyer",
            "isMainContact": true
        }
    })
}

fn b2b_mutation_first_hydrate_test_contact() -> Value {
    json!({
        "__typename": "CompanyContact",
        "id": B2B_HYDRATE_TEST_CONTACT_ID,
        "title": "Buyer",
        "locale": "en",
        "isMainContact": true,
        "customer": {
            "__typename": "Customer",
            "id": B2B_HYDRATE_TEST_CUSTOMER_ID,
            "firstName": "Hydrated",
            "lastName": "Buyer",
            "displayName": "Hydrated Buyer",
            "email": "buyer@example.com",
            "phone": Value::Null,
            "locale": "en",
            "defaultEmailAddress": { "emailAddress": "buyer@example.com" },
            "defaultPhoneNumber": Value::Null
        },
        "company": b2b_mutation_first_hydrate_test_company(),
        "roleAssignments": {
            "nodes": [{
                "__typename": "CompanyContactRoleAssignment",
                "id": B2B_HYDRATE_TEST_ROLE_ASSIGNMENT_ID,
                "companyContact": { "id": B2B_HYDRATE_TEST_CONTACT_ID, "title": "Buyer" },
                "companyLocation": { "id": B2B_HYDRATE_TEST_LOCATION_ID, "name": "Hydrated HQ" },
                "role": { "id": B2B_HYDRATE_TEST_ROLE_ID, "name": "Ordering only", "note": Value::Null }
            }],
            "pageInfo": { "hasNextPage": false, "endCursor": Value::Null }
        }
    })
}

fn b2b_mutation_first_hydrate_test_location() -> Value {
    json!({
        "__typename": "CompanyLocation",
        "id": B2B_HYDRATE_TEST_LOCATION_ID,
        "name": "Hydrated HQ",
        "externalId": "HYDRATED-HQ",
        "locale": "en",
        "phone": Value::Null,
        "billingAddress": {
            "__typename": "CompanyAddress",
            "id": B2B_HYDRATE_TEST_ADDRESS_ID,
            "address1": "1 Hydration Way",
            "city": "Toronto",
            "countryCode": "CA"
        },
        "shippingAddress": Value::Null,
        "taxSettings": { "taxRegistrationId": Value::Null, "taxExempt": false, "taxExemptions": [] },
        "company": b2b_mutation_first_hydrate_test_company(),
        "roleAssignments": {
            "nodes": [{
                "__typename": "CompanyContactRoleAssignment",
                "id": B2B_HYDRATE_TEST_ROLE_ASSIGNMENT_ID,
                "companyContact": { "id": B2B_HYDRATE_TEST_CONTACT_ID, "title": "Buyer" },
                "companyLocation": { "id": B2B_HYDRATE_TEST_LOCATION_ID, "name": "Hydrated HQ" },
                "role": { "id": B2B_HYDRATE_TEST_ROLE_ID, "name": "Ordering only", "note": Value::Null }
            }],
            "pageInfo": { "hasNextPage": false, "endCursor": Value::Null }
        },
        "staffMemberAssignments": {
            "nodes": [{
                "__typename": "CompanyLocationStaffMemberAssignment",
                "id": B2B_HYDRATE_TEST_STAFF_ASSIGNMENT_ID,
                "companyLocation": { "id": B2B_HYDRATE_TEST_LOCATION_ID, "name": "Hydrated HQ" },
                "staffMember": { "id": B2B_HYDRATE_TEST_STAFF_ID }
            }],
            "pageInfo": { "hasNextPage": false, "endCursor": Value::Null }
        }
    })
}

fn create_b2b_company(proxy: &mut DraftProxy, name: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCreateCompany($name: String!) {
          companyCreate(input: { company: { name: $name } }) {
            company { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "name": name }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["companyCreate"]["company"]["id"]
        .as_str()
        .expect("company id")
        .to_string()
}

fn b2b_company_location_hydrate_response(
    location_id: &str,
    location_name: &str,
    company_id: &str,
) -> Response {
    Response {
        status: 200,
        headers: Default::default(),
        body: json!({
            "data": {
                "companyLocation": b2b_company_location_hydrate_record(
                    location_id,
                    location_name,
                    company_id,
                )
            }
        }),
    }
}

fn b2b_mutation_targets_location_hydrate_response(
    location_id: &str,
    location_name: &str,
    company_id: &str,
) -> Response {
    Response {
        status: 200,
        headers: Default::default(),
        body: json!({
            "data": {
                "nodes": [b2b_company_location_hydrate_record(
                    location_id,
                    location_name,
                    company_id,
                )]
            }
        }),
    }
}

fn b2b_company_location_hydrate_record(
    location_id: &str,
    location_name: &str,
    company_id: &str,
) -> Value {
    json!({
        "__typename": "CompanyLocation",
        "id": location_id,
        "name": location_name,
        "externalId": Value::Null,
        "note": Value::Null,
        "locale": "en",
        "phone": Value::Null,
        "billingAddress": { "id": "gid://shopify/CompanyAddress/200", "address1": "Upstream Billing" },
        "shippingAddress": { "id": "gid://shopify/CompanyAddress/201", "address1": "Upstream Shipping" },
        "taxSettings": {
            "taxRegistrationId": Value::Null,
            "taxExempt": false,
            "taxExemptions": ["US_CA_RESELLER_EXEMPTION"]
        },
        "buyerExperienceConfiguration": {
            "editableShippingAddress": false,
            "checkoutToDraft": false,
            "paymentTermsTemplate": Value::Null,
            "deposit": Value::Null
        },
        "roleAssignments": { "nodes": [] },
        "staffMemberAssignments": { "nodes": [] },
        "company": {
            "id": company_id,
            "name": "Hydrated Company",
            "locations": { "nodes": [{ "id": location_id, "name": location_name }] },
            "contacts": { "nodes": [] },
            "contactRoles": { "nodes": [] },
            "mainContact": Value::Null
        }
    })
}

fn create_b2b_company_contact(proxy: &mut DraftProxy, company_id: &str, title: &str) -> String {
    let local_part: String = title
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .map(|character| character.to_ascii_lowercase())
        .collect();
    let email = format!(
        "{}@example.com",
        if local_part.is_empty() {
            "buyer"
        } else {
            local_part.as_str()
        }
    );
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCreateCompanyContact($companyId: ID!, $title: String!, $email: String!) {
          companyContactCreate(companyId: $companyId, input: { title: $title, email: $email }) {
            companyContact { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id, "title": title, "email": email }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companyContactCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["companyContactCreate"]["companyContact"]["id"]
        .as_str()
        .expect("company contact id")
        .to_string()
}

fn create_b2b_company_with_contact_and_role(proxy: &mut DraftProxy) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCreateCompanyWithContact {
          companyCreate(input: {
            company: { name: "Role Co" },
            companyContact: { title: "Buyer", email: "role-buyer@example.com" },
          }) {
            company { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({}),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companyCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["companyCreate"]["company"]["id"]
        .as_str()
        .expect("company id")
        .to_string()
}

fn create_b2b_location(proxy: &mut DraftProxy, company_id: &str, name: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCreateLocation($companyId: ID!, $name: String!) {
          companyLocationCreate(companyId: $companyId, input: { name: $name }) {
            companyLocation { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id, "name": name }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companyLocationCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["companyLocationCreate"]["companyLocation"]["id"]
        .as_str()
        .expect("location id")
        .to_string()
}

fn assert_b2b_bulk_limit_response(
    query: &str,
    variables: Value,
    root: &str,
    argument_field: &str,
    empty_result_fields: &[&str],
) {
    let mut proxy = snapshot_proxy();
    let state_before = state_snapshot(&proxy);

    let response = proxy.process_request(json_graphql_request(query, variables));
    assert_eq!(response.status, 200);
    let payload = &response.body["data"][root];
    for field in empty_result_fields {
        assert_eq!(payload[*field], json!([]), "{root}.{field}");
    }
    assert_eq!(
        payload["userErrors"],
        json!([b2b_bulk_limit_error(argument_field)])
    );
    assert_eq!(state_snapshot(&proxy), state_before);

    let log = log_snapshot(&proxy);
    let entry = log["entries"]
        .as_array()
        .expect("mutation log entries")
        .last()
        .expect("bulk limit log entry");
    assert_eq!(entry["status"], json!("failed"));
    assert_eq!(entry["interpreted"]["primaryRootField"], json!(root));
    assert_eq!(entry["stagedResourceIds"], json!([]));
}

fn b2b_test_ids(resource_type: &str, count: usize) -> Vec<String> {
    (0..count)
        .map(|index| format!("gid://shopify/{resource_type}/{}", index + 1))
        .collect()
}

fn b2b_contact_role_assign_inputs(count: usize) -> Vec<Value> {
    (0..count)
        .map(|index| {
            json!({
                "companyContactRoleId": format!("gid://shopify/CompanyContactRole/{}", index + 1),
                "companyLocationId": format!("gid://shopify/CompanyLocation/{}", index + 1)
            })
        })
        .collect()
}

fn b2b_location_role_assign_inputs(count: usize) -> Vec<Value> {
    (0..count)
        .map(|index| {
            json!({
                "companyContactId": format!("gid://shopify/CompanyContact/{}", index + 1),
                "companyContactRoleId": format!("gid://shopify/CompanyContactRole/{}", index + 1)
            })
        })
        .collect()
}

fn b2b_bulk_limit_error(argument_field: &str) -> Value {
    json!({
        "field": [argument_field],
        "message": "Exceeded max input size of 50. Consider using BulkOperation.",
        "code": "LIMIT_REACHED"
    })
}

fn read_b2b_first_contact_and_role(proxy: &mut DraftProxy, company_id: &str) -> (String, String) {
    let response = proxy.process_request(json_graphql_request(
        r#"
        query B2BFirstContactAndRole($id: ID!) {
          company(id: $id) {
            contacts(first: 5) { nodes { id } }
            contactRoles(first: 5) { nodes { id } }
          }
        }
        "#,
        json!({ "id": company_id }),
    ));
    assert_eq!(response.status, 200);
    let contact_id = response.body["data"]["company"]["contacts"]["nodes"][0]["id"]
        .as_str()
        .expect("contact id")
        .to_string();
    let role_id = response.body["data"]["company"]["contactRoles"]["nodes"][0]["id"]
        .as_str()
        .expect("role id")
        .to_string();
    (contact_id, role_id)
}

fn read_b2b_company_location_ids(proxy: &mut DraftProxy, company_id: &str) -> Vec<String> {
    let response = proxy.process_request(json_graphql_request(
        r#"
        query B2BCompanyLocationIds($id: ID!) {
          company(id: $id) {
            locations(first: 10) { nodes { id } }
          }
        }
        "#,
        json!({ "id": company_id }),
    ));
    assert_eq!(response.status, 200);
    response.body["data"]["company"]["locations"]["nodes"]
        .as_array()
        .expect("location nodes")
        .iter()
        .map(|location| location["id"].as_str().expect("location id").to_string())
        .collect()
}

fn assign_b2b_contact_role(
    proxy: &mut DraftProxy,
    contact_id: &str,
    role_id: &str,
    location_id: &str,
) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignContactRole(
          $companyContactId: ID!
          $companyContactRoleId: ID!
          $companyLocationId: ID!
        ) {
          companyContactAssignRole(
            companyContactId: $companyContactId
            companyContactRoleId: $companyContactRoleId
            companyLocationId: $companyLocationId
          ) {
            companyContactRoleAssignment { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "companyContactId": contact_id,
            "companyContactRoleId": role_id,
            "companyLocationId": location_id
        }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companyContactAssignRole"]["userErrors"],
        json!([])
    );
    response.body["data"]["companyContactAssignRole"]["companyContactRoleAssignment"]["id"]
        .as_str()
        .expect("role assignment id")
        .to_string()
}

fn create_b2b_location_with_shared_address(proxy: &mut DraftProxy, company_id: &str) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCreateSharedAddressLocation($companyId: ID!) {
          companyLocationCreate(
            companyId: $companyId,
            input: {
              name: "Shared HQ",
              shippingAddress: { address1: "Shared Street", city: "Denver", countryCode: "US" },
              billingSameAsShipping: true
            }
          ) {
            companyLocation { id }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "companyId": company_id }),
    ));
    assert_eq!(response.status, 200);
    assert_eq!(
        response.body["data"]["companyLocationCreate"]["userErrors"],
        json!([])
    );
    response.body["data"]["companyLocationCreate"]["companyLocation"]["id"]
        .as_str()
        .expect("location id")
        .to_string()
}
