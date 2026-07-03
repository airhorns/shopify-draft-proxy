use super::common::*;
use pretty_assertions::assert_eq;

#[test]
fn b2b_tax_settings_update_tail_helpers_port_old_gleam_tests() {
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
        mutation RustB2BTaxSettingsInvalidEnumLiteral($locationId: ID!) {
          companyLocationTaxSettingsUpdate(companyLocationId: $locationId, exemptionsToAssign: [NOT_A_REAL_EXEMPTION]) {
            companyLocation { id taxSettings { taxExemptions } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id }),
    ));
    assert_eq!(invalid_literal.status, 200);
    assert_eq!(
        invalid_literal.body["errors"][0]["extensions"]["code"],
        json!("argumentLiteralsIncompatible")
    );
    assert!(invalid_literal.body["errors"][0]["message"]
        .as_str()
        .is_some_and(|message| message.contains("NOT_A_REAL_EXEMPTION")
            && message.contains("CA_STATUS_CARD_EXEMPTION")));
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
fn b2b_location_buyer_experience_configuration_update_tail_helpers_port_old_gleam_tests() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "Buyer Experience Co");
    let location_id = create_b2b_location(&mut proxy, &company_id, "Buyer Experience Branch");

    let empty = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BLocationBuyerExperienceConfigurationUpdate($locationId: ID!) {
          companyLocationUpdate(companyLocationId: $locationId, input: { buyerExperienceConfiguration: {} }) {
            companyLocation { id }
            userErrors { field message code detail }
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
                "code": "INVALID_INPUT",
                "detail": "buyer_experience_configuration_empty"
            }]
        })
    );

    let deposit_without_terms = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BLocationBuyerExperienceConfigurationUpdate($locationId: ID!) {
          companyLocationUpdate(companyLocationId: $locationId, input: { buyerExperienceConfiguration: { deposit: { percentage: 50.0 } } }) {
            companyLocation { id }
            userErrors { field message code detail }
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
                "code": "INVALID",
                "detail": "deposit_without_payment_terms"
            }]
        })
    );

    let deposit_disabled = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BLocationBuyerExperienceConfigurationDepositDisabled($locationId: ID!) {
          companyLocationUpdate(companyLocationId: $locationId, input: { buyerExperienceConfiguration: { paymentTermsTemplateId: "gid://shopify/PaymentTermsTemplate/4", deposit: { percentage: 50.0 } } }) {
            companyLocation { id }
            userErrors { field message code detail }
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
                "code": "INVALID",
                "detail": "deposit_not_enabled"
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
fn b2b_company_identity_validation_tail_helpers_port_old_gleam_tests() {
    let mut proxy = snapshot_proxy();

    let long_name = "x".repeat(300);
    let long_name_response = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyNameValidation($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id name }
            userErrors { field message code detail }
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
                "code": "TOO_LONG",
                "detail": Value::Null
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
            userErrors { field message code detail }
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
                "code": "TOO_LONG",
                "detail": Value::Null
            }]
        })
    );

    let invalid_external = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyExternalIdCreateValidation($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id externalId }
            userErrors { field message code detail }
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
                "code": "INVALID",
                "detail": "external_id_contains_invalid_chars"
            }]
        })
    );

    let first = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyExternalIdCreateValidation($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id externalId }
            userErrors { field message code detail }
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
            userErrors { field message code detail }
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
                "code": "TAKEN",
                "detail": "duplicate_external_id"
            }]
        })
    );

    let second = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyExternalIdCreateValidation($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id externalId }
            userErrors { field message code detail }
          }
        }
        "#,
        json!({ "company": { "name": "Second", "externalId": "ACME-2" } }),
    ));
    let second_id = second.body["data"]["companyCreate"]["company"]["id"].clone();

    let self_update = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyExternalIdUpdateValidation($id: ID!, $input: CompanyUpdateInput!) {
          companyUpdate(companyId: $id, input: $input) {
            company { id externalId }
            userErrors { field message code detail }
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
        mutation RustB2BCompanyExternalIdUpdateValidation($id: ID!, $input: CompanyUpdateInput!) {
          companyUpdate(companyId: $id, input: $input) {
            company { id externalId }
            userErrors { field message code detail }
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
                "code": "TAKEN",
                "detail": "duplicate_external_id"
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
            "email": "order-customer-b2b@example.com",
            "purchasingEntity": {
                "purchasingCompany": { "companyId": blocked_id }
            },
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
fn b2b_company_update_immutable_and_note_validation_tail_helpers_port_old_gleam_tests() {
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
        mutation RustB2BCompanyCustomerSinceUpdate($id: ID!, $input: CompanyUpdateInput!) {
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
        mutation RustB2BCompanyCustomerSinceUpdate($id: ID!, $input: CompanyUpdateInput!) {
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
        mutation RustB2BCompanyCustomerSinceUpdate($id: ID!, $input: CompanyUpdateInput!) {
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

    let invalid_note = format!("<script>{}</script>", "x".repeat(6000));
    let note_reject = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyNoteValidation($id: ID!, $input: CompanyUpdateInput!) {
          companyUpdate(companyId: $id, input: $input) {
            company { id note }
            userErrors { field message code detail }
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
        json!([
            {
                "field": ["input", "notes"],
                "message": "Note contains HTML tags",
                "code": "INVALID",
                "detail": "contains_html_tags"
            },
            {
                "field": ["input", "notes"],
                "message": "Notes is too long (maximum is 5000 characters)",
                "code": "TOO_LONG",
                "detail": Value::Null
            }
        ])
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
            companyContact { id title customer { firstName lastName email phone } }
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
                "phone": "+14155550101",
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
                    "phone": "+14155550101"
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
            companyContact { id }
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
        json!([{
            "field": ["input", "title"],
            "message": "Title contains HTML tags",
            "code": "CONTAINS_HTML_TAGS"
        }])
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
            companyContact {
              id
              roleAssignments(first: 5) { nodes { id } }
            }
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
        revoke.body["data"]["companyContactRevokeRole"]["companyContact"]["id"],
        contact_id
    );
    assert_eq!(
        revoke.body["data"]["companyContactRevokeRole"]["companyContact"]["roleAssignments"]
            ["nodes"],
        json!([])
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
              locations(first: 5) { nodes { id name billingSameAsShipping shippingAddress { id address1 } billingAddress { id address1 } } }
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
    assert_eq!(first_location["billingSameAsShipping"], json!(true));
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
            billingSameAsShipping
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
            billingSameAsShipping
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
            "billingSameAsShipping": false,
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
            companyContact { id }
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
            "companyContact": Value::Null,
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
        mutation B2BAssignRoles($locationId: ID!, $roles: [CompanyLocationAssignRolesInput!]!) {
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
                "message": "Resource requested does not exist.",
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
            companyContactRole: { name: "Location admin" }
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
