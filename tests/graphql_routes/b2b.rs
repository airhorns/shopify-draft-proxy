use super::common::*;
use pretty_assertions::assert_eq;

#[test]
fn b2b_tax_settings_update_tail_helpers_port_old_gleam_tests() {
    let mut proxy = snapshot_proxy();
    let location_id = "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic";

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
            "companyLocation": Value::Null,
            "userErrors": [{
                "field": ["companyLocationId"],
                "message": "No tax settings input was provided",
                "code": "NO_INPUT"
            }]
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
          companyLocationTaxSettingsUpdate(companyLocationId: "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic", exemptionsToAssign: [NOT_A_REAL_EXEMPTION]) {
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

    let log = proxy.get_log_snapshot();
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
fn b2b_location_buyer_experience_configuration_update_tail_helpers_port_old_gleam_tests() {
    let mut proxy = snapshot_proxy();
    let location_id = "gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic";

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
                "taxSettings": { "taxExempt": true },
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

    let entries = proxy.get_log_snapshot()["entries"]
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
                "message": "Company name is too long",
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
                "message": "External ID is too long",
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
                "message": "External ID contains invalid characters",
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
                "message": "External ID has already been taken",
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
                "message": "External ID has already been taken",
                "code": "TAKEN",
                "detail": "duplicate_external_id"
            }]
        })
    );
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
                "message": "Note is too long",
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

    let entries = proxy.get_log_snapshot()["entries"]
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

    let entries = proxy.get_log_snapshot()["entries"]
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
fn b2b_contact_role_assign_and_revoke_stage_relationships_with_indexed_errors() {
    let mut proxy = snapshot_proxy();
    let company = proxy.process_request(json_graphql_request(
        r#"
        mutation RustB2BCompanyContactRolesCompany($company: CompanyInput!) {
          companyCreate(input: { company: $company }) {
            company { id contactRoles(first: 5) { nodes { id name } } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "company": { "name": "Contact Roles Co" } }),
    ));
    let company_id = company.body["data"]["companyCreate"]["company"]["id"].clone();
    let role_id =
        company.body["data"]["companyCreate"]["company"]["contactRoles"]["nodes"][0]["id"].clone();
    let location_id = json!("gid://shopify/CompanyLocation/4?shopify-draft-proxy=synthetic");

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
fn b2b_fixture_backed_reads_cover_customer_since_and_assignment_nodes() {
    let mut proxy = configured_proxy(ReadMode::LiveHybrid, None);

    let company = proxy.process_request(json_graphql_request(
        r#"
        query B2BCustomerSinceCompanyRead($companyId: ID!) {
          company(id: $companyId) {
            name
            customerSince
          }
        }
        "#,
        json!({ "companyId": "gid://shopify/Company/7681462450" }),
    ));
    assert_eq!(
        company.body["data"]["company"],
        json!({
            "name": "HAR-760 customerSince 1778017011251",
            "customerSince": "2024-01-01T00:00:00Z"
        })
    );

    let nodes = proxy.process_request(json_graphql_request(
        r#"
        query B2BContactLocationAssignmentsNodeRead($ids: [ID!]!) {
          nodes(ids: $ids) {
            ... on CompanyAddress { id address1 city countryCode }
            ... on CompanyContactRoleAssignment {
              id
              companyContact { id title }
              role { id name }
              companyLocation { id name }
            }
            ... on CompanyContact { id title }
            ... on CompanyContactRole { id name }
            ... on CompanyLocation { id name }
          }
        }
        "#,
        json!({
            "ids": [
                "gid://shopify/CompanyAddress/9348383026",
                "gid://shopify/CompanyContactRoleAssignment/44647547186",
                "gid://shopify/CompanyContact/10149003570",
                "gid://shopify/CompanyLocation/8247738674",
                "gid://shopify/CompanyContactRole/10668638514"
            ]
        }),
    ));

    assert_eq!(
        nodes.body["data"]["nodes"],
        json!([
            {
                "id": "gid://shopify/CompanyAddress/9348383026",
                "address1": "446 Assignment Way",
                "city": "Toronto",
                "countryCode": "CA"
            },
            {
                "id": "gid://shopify/CompanyContactRoleAssignment/44647547186",
                "companyContact": {
                    "id": "gid://shopify/CompanyContact/10149003570",
                    "title": "Lead buyer"
                },
                "role": {
                    "id": "gid://shopify/CompanyContactRole/10668638514",
                    "name": "Location admin"
                },
                "companyLocation": {
                    "id": "gid://shopify/CompanyLocation/8247738674",
                    "name": "HAR-446 B2B assignment 1778015458844 Single assignment updated"
                }
            },
            {
                "id": "gid://shopify/CompanyContact/10149003570",
                "title": "Lead buyer"
            },
            {
                "id": "gid://shopify/CompanyLocation/8247738674",
                "name": "HAR-446 B2B assignment 1778015458844 Single assignment updated"
            },
            {
                "id": "gid://shopify/CompanyContactRole/10668638514",
                "name": "Location admin"
            }
        ])
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
                    "name": null,
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
    assert_eq!(first_location["name"], json!("123 Main"));
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
            input: { shippingAddress: { address1: "456 Side", city: "Austin", countryCode: "US" } }
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

    let log = proxy.get_log_snapshot();
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
    assert_eq!(
        read.body["data"]["company"]["locations"]["nodes"],
        json!([{ "id": "gid://shopify/CompanyLocation/6?shopify-draft-proxy=synthetic" }])
    );
    assert_eq!(
        read.body["data"]["companyLocations"]["nodes"],
        json!([{ "id": "gid://shopify/CompanyLocation/6?shopify-draft-proxy=synthetic" }])
    );
}

#[test]
fn b2b_staff_assign_remove_validates_per_index_dedups_and_caps() {
    let mut proxy = snapshot_proxy();
    let company_id = create_b2b_company(&mut proxy, "Staff Co");
    let location_id = create_b2b_location(&mut proxy, &company_id, "Staff HQ");

    let assign = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BAssignStaff($locationId: ID!, $staff: [ID!]!) {
          companyLocationAssignStaffMembers(companyLocationId: $locationId, staffMemberIds: $staff) {
            companyLocationStaffMemberAssignments {
              id
              staffMember { id }
              companyLocation { id }
            }
            userErrors { field message code }
          }
        }
        "#,
        json!({
            "locationId": location_id,
            "staff": [
                "gid://shopify/StaffMember/1",
                "gid://shopify/StaffMember/1",
                "gid://shopify/StaffMember/missing"
            ]
        }),
    ));
    assert_eq!(assign.status, 200);
    assert_eq!(
        assign.body["data"]["companyLocationAssignStaffMembers"]
            ["companyLocationStaffMemberAssignments"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        assign.body["data"]["companyLocationAssignStaffMembers"]["userErrors"],
        json!([{
            "field": ["staffMemberIds", "2"],
            "message": "Resource requested does not exist.",
            "code": "RESOURCE_NOT_FOUND"
        }])
    );
    let assignment_id = assign.body["data"]["companyLocationAssignStaffMembers"]
        ["companyLocationStaffMemberAssignments"][0]["id"]
        .as_str()
        .expect("staff assignment id")
        .to_string();

    let cap_ids = (2..=12)
        .map(|id| format!("gid://shopify/StaffMember/{id}"))
        .collect::<Vec<_>>();
    let cap = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BStaffCap($locationId: ID!, $staff: [ID!]!) {
          companyLocationAssignStaffMembers(companyLocationId: $locationId, staffMemberIds: $staff) {
            companyLocationStaffMemberAssignments { id staffMember { id } }
            userErrors { field message code }
          }
        }
        "#,
        json!({ "locationId": location_id, "staff": cap_ids }),
    ));
    assert_eq!(
        cap.body["data"]["companyLocationAssignStaffMembers"]
            ["companyLocationStaffMemberAssignments"]
            .as_array()
            .unwrap()
            .len(),
        9
    );
    assert_eq!(
        cap.body["data"]["companyLocationAssignStaffMembers"]["userErrors"][0],
        json!({
            "field": ["staffMemberIds", "9"],
            "message": "Cannot assign more than 10 staff members to a company location.",
            "code": "LIMIT_REACHED"
        })
    );

    let remove = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BRemoveStaff($ids: [ID!]!) {
          companyLocationRemoveStaffMembers(companyLocationStaffMemberAssignmentIds: $ids) {
            deletedCompanyLocationStaffMemberAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "ids": [assignment_id, "gid://shopify/CompanyLocationStaffMemberAssignment/999"] }),
    ));
    assert_eq!(
        remove.body["data"]["companyLocationRemoveStaffMembers"],
        json!({
            "deletedCompanyLocationStaffMemberAssignmentIds": [assignment_id],
            "userErrors": [{
                "field": ["companyLocationStaffMemberAssignmentIds", "1"],
                "message": "Resource requested does not exist.",
                "code": "RESOURCE_NOT_FOUND"
            }]
        })
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
        mutation B2BRevokeRoles($roles: [ID!]!) {
          companyLocationRevokeRoles(rolesToRevoke: $roles) {
            revokedRoleAssignmentIds
            userErrors { field message code }
          }
        }
        "#,
        json!({ "roles": [assignment_id, "gid://shopify/CompanyContactRoleAssignment/999"] }),
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

fn create_b2b_company_with_contact_and_role(proxy: &mut DraftProxy) -> String {
    let response = proxy.process_request(json_graphql_request(
        r#"
        mutation B2BCreateCompanyWithContact {
          companyCreate(input: {
            company: { name: "Role Co" },
            companyContact: { title: "Buyer" },
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
