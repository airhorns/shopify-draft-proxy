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
