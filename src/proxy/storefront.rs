use super::b2b_customers::{
    customer_address_cursor, customer_address_dedup_key, customer_address_input_node,
    customer_address_nodes, customer_rebuild_addresses,
};
use super::*;
use crate::graphql::operation_directive_invocations;
use base64::Engine as _;
use sha2::{Digest, Sha256};

const STOREFRONT_FIRST_SLICE_VERSION: &str = "2026-04";
const STOREFRONT_FIRST_SLICE_ROOTS: &[&str] = &[
    "shop",
    "localization",
    "locations",
    "paymentSettings",
    "publicApiVersions",
    "product",
    "productByHandle",
    "productRecommendations",
    "productTags",
    "productTypes",
    "products",
];
const STOREFRONT_CONTENT_ROOTS: &[&str] = &[
    "article",
    "articles",
    "blog",
    "blogByHandle",
    "blogs",
    "page",
    "pageByHandle",
    "pages",
];
const STOREFRONT_LOCAL_CONTENT_ROOTS: &[&str] = &[
    "article",
    "articles",
    "blog",
    "blogByHandle",
    "blogs",
    "menu",
    "page",
    "pageByHandle",
    "pages",
    "sitemap",
    "urlRedirects",
];
const STOREFRONT_CUSTOM_DATA_ROOTS: &[&str] = &["metaobject", "metaobjects"];
const STOREFRONT_COLLECTION_ROOTS: &[&str] = &["collection", "collectionByHandle", "collections"];
const STOREFRONT_DISCOVERY_ROOTS: &[&str] = &["node", "nodes", "search", "predictiveSearch"];
const STOREFRONT_CAPTURED_COLLECTION_DEFAULT_ORDER_FIELD: &str =
    "__storefrontCapturedDefaultProductOrder";
pub(in crate::proxy) const STOREFRONT_CUSTOMER_AUTH_MUTATION_ROOTS: &[&str] = &[
    "customerCreate",
    "customerAccessTokenCreate",
    "customerAccessTokenRenew",
    "customerAccessTokenDelete",
    "customerActivate",
    "customerActivateByUrl",
    "customerRecover",
    "customerReset",
    "customerResetByUrl",
    "customerAccessTokenCreateWithMultipass",
    "customerUpdate",
    "customerAddressCreate",
    "customerAddressUpdate",
    "customerAddressDelete",
    "customerDefaultAddressUpdate",
];
pub(in crate::proxy) const STOREFRONT_CART_MUTATION_ROOTS: &[&str] = &[
    "cartCreate",
    "cartLinesAdd",
    "cartLinesUpdate",
    "cartLinesRemove",
    "cartAttributesUpdate",
    "cartNoteUpdate",
    "cartBuyerIdentityUpdate",
    "cartDiscountCodesUpdate",
    "cartGiftCardCodesAdd",
    "cartGiftCardCodesRemove",
    "cartGiftCardCodesUpdate",
    "cartMetafieldsSet",
    "cartMetafieldDelete",
    "cartDeliveryAddressesAdd",
    "cartDeliveryAddressesUpdate",
    "cartDeliveryAddressesRemove",
    "cartDeliveryAddressesReplace",
    "cartSelectedDeliveryOptionsUpdate",
];
const STOREFRONT_DEFAULT_CONTEXT_KEY: &str = "country=*;language=*";
const STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD: &str = "__storefrontPasswordFingerprint";
const STOREFRONT_CUSTOMER_RESET_TOKEN_HASH_FIELD: &str = "__storefrontResetTokenHash";
const STOREFRONT_CUSTOMER_RESET_REQUESTED_AT_FIELD: &str = "__storefrontResetRequestedAt";
const STOREFRONT_CUSTOMER_ACTIVATION_TOKEN_FIELD: &str = "__proxyAccountActivationToken";

const STOREFRONT_FIRST_SLICE_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/storefront/storefront-first-slice-hydrate.graphql");
const STOREFRONT_FIRST_SLICE_CONTEXT_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/storefront/storefront-first-slice-hydrate-context.graphql"
);
const STOREFRONT_ENRICHMENT_TAXONOMY_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/storefront/storefront-enrichment-taxonomy-hydrate.graphql"
);
const STOREFRONT_ENRICHMENT_CONTEXT_HYDRATE_QUERY: &str = include_str!(
    "../../config/parity-requests/storefront/storefront-enrichment-context-hydrate.graphql"
);
const STOREFRONT_MENU_HYDRATE_QUERY: &str =
    include_str!("../../config/parity-requests/storefront/storefront-content-menu-hydrate.graphql");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorefrontContentKind {
    Blog,
    Page,
    Article,
}

#[derive(Clone)]
enum StorefrontSearchItem {
    Product(Box<ProductRecord>),
    Article(Value),
    Page(Value),
}

#[derive(Clone, Copy)]
enum StorefrontProductTaxonomyKind {
    Tag,
    ProductType,
}

#[derive(Clone)]
pub(in crate::proxy) struct StorefrontVariantPricing {
    pub(in crate::proxy) price: String,
    pub(in crate::proxy) compare_at_price: Option<String>,
    pub(in crate::proxy) currency_code: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::proxy) struct StorefrontRequestContext {
    pub(in crate::proxy) country: Option<String>,
    pub(in crate::proxy) language: Option<String>,
    pub(in crate::proxy) preferred_location_id: Option<String>,
    pub(in crate::proxy) buyer_customer_access_token: Option<String>,
    pub(in crate::proxy) buyer_company_location_id: Option<String>,
    pub(in crate::proxy) uses_enrichment_context: bool,
}

impl StorefrontRequestContext {
    fn key(&self) -> String {
        match (
            self.country.as_deref(),
            self.language.as_deref(),
            self.buyer_company_location_id.as_deref(),
        ) {
            (None, None, None) => STOREFRONT_DEFAULT_CONTEXT_KEY.to_string(),
            (country, language, None) => format!(
                "country={};language={}",
                country.unwrap_or("*"),
                language.unwrap_or("*")
            ),
            (country, language, Some(company_location_id)) => format!(
                "country={};language={};companyLocation={}",
                country.unwrap_or("*"),
                language.unwrap_or("*"),
                company_location_id
            ),
        }
    }

    fn has_in_context_values(&self) -> bool {
        self.country.is_some() || self.language.is_some()
    }

    pub(in crate::proxy) fn invalid_buyer_token(&self, proxy: &DraftProxy) -> bool {
        self.buyer_customer_access_token
            .as_deref()
            .is_some_and(|token| {
                proxy
                    .storefront_customer_id_for_access_token(token)
                    .is_none()
            })
    }
}

struct StorefrontCustomerAuthOutcome {
    value: Value,
    errors: Vec<Value>,
}

pub(in crate::proxy) struct StorefrontCustomerAuthLogDetails<'a> {
    pub status: &'a str,
    pub execution: &'a str,
    pub notes: &'a str,
}

impl DraftProxy {
    fn storefront_customer_query_root(
        &self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let token =
            resolved_string_field(&field.arguments, "customerAccessToken").unwrap_or_default();
        let customer = self
            .storefront_customer_id_for_access_token(&token)
            .and_then(|customer_id| {
                self.storefront_customer_by_id(&customer_id)
                    .map(|customer| {
                        self.storefront_customer_selected_json(
                            &customer_id,
                            &customer,
                            &field.selection,
                        )
                    })
            });
        StorefrontCustomerAuthOutcome {
            value: customer.unwrap_or(Value::Null),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_mutation_root(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        match field.name.as_str() {
            "customerCreate" => self.storefront_customer_create(field),
            "customerAccessTokenCreate" => self.storefront_customer_access_token_create(field),
            "customerAccessTokenRenew" => self.storefront_customer_access_token_renew(field),
            "customerAccessTokenDelete" => self.storefront_customer_access_token_delete(field),
            "customerActivate" => self.storefront_customer_activate(field),
            "customerActivateByUrl" => self.storefront_customer_activate_by_url(field),
            "customerRecover" => self.storefront_customer_recover(field),
            "customerReset" => self.storefront_customer_reset(field),
            "customerResetByUrl" => self.storefront_customer_reset_by_url(field),
            "customerAccessTokenCreateWithMultipass" => {
                self.storefront_customer_access_token_create_with_multipass(field)
            }
            "customerUpdate" => self.storefront_customer_update(field),
            "customerAddressCreate" => self.storefront_customer_address_create(field),
            "customerAddressUpdate" => self.storefront_customer_address_update(field),
            "customerAddressDelete" => self.storefront_customer_address_delete(field),
            "customerDefaultAddressUpdate" => {
                self.storefront_customer_default_address_update(field)
            }
            _ => StorefrontCustomerAuthOutcome {
                value: Value::Null,
                errors: Vec::new(),
            },
        }
    }

    fn storefront_customer_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let email = resolved_string_field(&input, "email").unwrap_or_default();
        let password = resolved_string_field(&input, "password").unwrap_or_default();
        let normalized_email = storefront_customer_email_key(&email);
        let mut errors = Vec::new();
        if password.is_empty() {
            errors.push(storefront_customer_user_error(
                ["input", "password"],
                "Password can't be blank",
                Some("BLANK"),
            ));
        }
        if !storefront_email_looks_valid(&email) {
            errors.push(storefront_customer_user_error(
                ["input", "email"],
                "Email is invalid",
                Some("INVALID"),
            ));
        }
        if self
            .storefront_customer_id_by_email(&normalized_email)
            .is_some()
        {
            errors.push(storefront_customer_user_error(
                ["input", "email"],
                "Email has already been taken",
                Some("TAKEN"),
            ));
        }
        for (field_name, message, code) in [
            (
                "firstName",
                "First name cannot contain HTML tags",
                "CONTAINS_HTML_TAGS",
            ),
            (
                "lastName",
                "Last name cannot contain HTML tags",
                "CONTAINS_HTML_TAGS",
            ),
        ] {
            if resolved_string_field(&input, field_name)
                .is_some_and(|value| storefront_customer_contains_html_tag(&value))
            {
                errors.push(storefront_customer_user_error(
                    ["input", field_name],
                    message,
                    Some(code),
                ));
            }
        }
        if !errors.is_empty() {
            return StorefrontCustomerAuthOutcome {
                value: selected_json(
                    &storefront_customer_payload(Value::Null, Value::Null, errors),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        }

        let id = self.next_synthetic_gid("Customer");
        let timestamp = self.next_product_timestamp();
        let accepts_marketing = resolved_bool_field(&input, "acceptsMarketing").unwrap_or(false);
        let first_name =
            resolved_string_field(&input, "firstName").filter(|value| !value.is_empty());
        let last_name = resolved_string_field(&input, "lastName").filter(|value| !value.is_empty());
        let phone = resolved_string_field(&input, "phone").filter(|value| !value.is_empty());
        let mut customer = storefront_customer_shared_record(
            &id,
            first_name.as_deref(),
            last_name.as_deref(),
            &email,
            phone.as_deref(),
            accepts_marketing,
            &timestamp,
        );
        customer[STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD] =
            json!(storefront_password_fingerprint(&id, &password));
        self.store
            .staged
            .customers
            .insert(id.clone(), customer.clone());
        self.store
            .staged
            .locally_created_customer_ids
            .insert(id.clone());
        self.store
            .staged
            .storefront_customer_email_index
            .insert(normalized_email, id);

        StorefrontCustomerAuthOutcome {
            value: selected_json(
                &storefront_customer_payload(
                    storefront_customer_json(&customer),
                    Value::Null,
                    Vec::new(),
                ),
                &field.selection,
            ),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_access_token_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let email = resolved_string_field(&input, "email").unwrap_or_default();
        let password = resolved_string_field(&input, "password").unwrap_or_default();
        let payload = match self
            .storefront_customer_id_by_email(&storefront_customer_email_key(&email))
            .and_then(|customer_id| self.storefront_customer_by_id(&customer_id))
        {
            Some(customer) if storefront_customer_password_matches(&customer, &password) => {
                if storefront_customer_state(&customer) == "DISABLED" {
                    storefront_customer_token_payload(
                        Value::Null,
                        vec![storefront_customer_user_error(
                            Value::Null,
                            "Customer is disabled",
                            Some("CUSTOMER_DISABLED"),
                        )],
                    )
                } else {
                    let customer_id = customer["id"].as_str().unwrap_or_default().to_string();
                    let token = self.issue_storefront_customer_access_token(&customer_id);
                    storefront_customer_token_payload(token, Vec::new())
                }
            }
            _ => storefront_customer_token_payload(
                Value::Null,
                vec![storefront_customer_user_error(
                    Value::Null,
                    "Unidentified customer",
                    Some("UNIDENTIFIED_CUSTOMER"),
                )],
            ),
        };

        StorefrontCustomerAuthOutcome {
            value: selected_json(&payload, &field.selection),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_access_token_renew(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let token =
            resolved_string_field(&field.arguments, "customerAccessToken").unwrap_or_default();
        let token_hash = storefront_token_hash(&token);
        let payload = if self.storefront_access_token_is_active(&token_hash) {
            let expires_at = self.store.staged.storefront_customer_access_tokens[&token_hash]
                ["expiresAt"]
                .clone();
            json!({
                "customerAccessToken": {
                    "accessToken": token,
                    "expiresAt": expires_at
                },
                "userErrors": []
            })
        } else {
            json!({
                "customerAccessToken": null,
                "userErrors": [{
                    "field": ["customerAccessToken"],
                    "message": "access token does not exist"
                }]
            })
        };
        StorefrontCustomerAuthOutcome {
            value: selected_json(&payload, &field.selection),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_access_token_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let token =
            resolved_string_field(&field.arguments, "customerAccessToken").unwrap_or_default();
        let token_hash = storefront_token_hash(&token);
        if !self.storefront_access_token_is_active(&token_hash) {
            return StorefrontCustomerAuthOutcome {
                value: Value::Null,
                errors: vec![storefront_access_denied_error(&field.response_key)],
            };
        }
        let token_id =
            self.store.staged.storefront_customer_access_tokens[&token_hash]["id"].clone();
        if let Some(record) = self
            .store
            .staged
            .storefront_customer_access_tokens
            .get_mut(&token_hash)
        {
            record["revoked"] = json!(true);
        }
        let payload = json!({
            "deletedAccessToken": token,
            "deletedCustomerAccessTokenId": token_id,
            "userErrors": []
        });
        StorefrontCustomerAuthOutcome {
            value: selected_json(&payload, &field.selection),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_activate(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let customer_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let activation_token = resolved_string_field(&input, "activationToken").unwrap_or_default();
        let password = resolved_string_field(&input, "password").unwrap_or_default();
        self.storefront_activate_customer_with_token(
            field,
            &customer_id,
            &activation_token,
            &password,
            ["input"],
        )
    }

    fn storefront_customer_activate_by_url(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let activation_url =
            resolved_string_field(&field.arguments, "activationUrl").unwrap_or_default();
        let password = resolved_string_field(&field.arguments, "password").unwrap_or_default();
        let Some((customer_id, token)) =
            self.storefront_customer_activation_url_parts(&activation_url)
        else {
            return StorefrontCustomerAuthOutcome {
                value: selected_json(
                    &storefront_customer_activation_payload(
                        Value::Null,
                        Value::Null,
                        vec![storefront_customer_user_error(
                            ["activationUrl"],
                            "Invalid activation url",
                            Some("INVALID"),
                        )],
                        false,
                    ),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        self.storefront_activate_customer_with_token(
            field,
            &customer_id,
            &token,
            &password,
            ["activationUrl"],
        )
    }

    fn storefront_activate_customer_with_token<const N: usize>(
        &mut self,
        field: &RootFieldSelection,
        customer_id: &str,
        activation_token: &str,
        password: &str,
        invalid_field: [&str; N],
    ) -> StorefrontCustomerAuthOutcome {
        let Some(mut customer) = self.storefront_customer_by_id(customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: selected_json(
                    &storefront_customer_activation_payload(
                        Value::Null,
                        Value::Null,
                        vec![storefront_customer_user_error(
                            invalid_field.to_vec(),
                            "Invalid activation token",
                            Some("TOKEN_INVALID"),
                        )],
                        true,
                    ),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        if storefront_customer_state(&customer) == "ENABLED" {
            return StorefrontCustomerAuthOutcome {
                value: selected_json(
                    &storefront_customer_activation_payload(
                        Value::Null,
                        Value::Null,
                        vec![storefront_customer_user_error(
                            Value::Null,
                            "Customer already enabled",
                            Some("ALREADY_ENABLED"),
                        )],
                        true,
                    ),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        }
        if activation_token != storefront_customer_activation_token_for_id(customer_id)
            && customer
                .get(STOREFRONT_CUSTOMER_ACTIVATION_TOKEN_FIELD)
                .and_then(Value::as_str)
                != Some(activation_token)
        {
            return StorefrontCustomerAuthOutcome {
                value: selected_json(
                    &storefront_customer_activation_payload(
                        Value::Null,
                        Value::Null,
                        vec![storefront_customer_user_error(
                            invalid_field.to_vec(),
                            "Invalid activation token",
                            Some("TOKEN_INVALID"),
                        )],
                        true,
                    ),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        }

        customer["state"] = json!("ENABLED");
        customer["updatedAt"] = json!(self.next_product_timestamp());
        customer[STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD] =
            json!(storefront_password_fingerprint(customer_id, password));
        self.store
            .staged
            .customers
            .stage(customer_id.to_string(), customer.clone());
        if let Some(email) = customer.get("email").and_then(Value::as_str) {
            self.store.staged.storefront_customer_email_index.insert(
                storefront_customer_email_key(email),
                customer_id.to_string(),
            );
        }
        let token = self.issue_storefront_customer_access_token(customer_id);
        StorefrontCustomerAuthOutcome {
            value: selected_json(
                &storefront_customer_activation_payload(
                    storefront_customer_json(&customer),
                    token,
                    Vec::new(),
                    true,
                ),
                &field.selection,
            ),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_recover(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let email = resolved_string_field(&field.arguments, "email").unwrap_or_default();
        let payload = if let Some(customer_id) =
            self.storefront_customer_id_by_email(&storefront_customer_email_key(&email))
        {
            if let Some(mut customer) = self.storefront_customer_by_id(&customer_id) {
                let reset_token = self.next_storefront_customer_reset_token(&customer_id);
                customer[STOREFRONT_CUSTOMER_RESET_TOKEN_HASH_FIELD] =
                    json!(storefront_token_hash(&reset_token));
                customer[STOREFRONT_CUSTOMER_RESET_REQUESTED_AT_FIELD] =
                    json!(self.next_product_timestamp());
                self.store.staged.customers.stage(customer_id, customer);
            }
            json!({ "customerUserErrors": [], "userErrors": [] })
        } else {
            let errors = vec![storefront_customer_user_error(
                ["email"],
                "Could not find customer",
                Some("UNIDENTIFIED_CUSTOMER"),
            )];
            json!({
                "customerUserErrors": errors,
                "userErrors": storefront_user_errors_without_code(&errors)
            })
        };
        StorefrontCustomerAuthOutcome {
            value: selected_json(&payload, &field.selection),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_reset(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let customer_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "input").unwrap_or_default();
        let reset_token = resolved_string_field(&input, "resetToken").unwrap_or_default();
        let password = resolved_string_field(&input, "password").unwrap_or_default();
        self.storefront_reset_customer_with_token(
            field,
            &customer_id,
            &reset_token,
            &password,
            true,
        )
    }

    fn storefront_customer_reset_by_url(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let reset_url = resolved_string_field(&field.arguments, "resetUrl").unwrap_or_default();
        let password = resolved_string_field(&field.arguments, "password").unwrap_or_default();
        let Some((customer_id, token)) = self.storefront_customer_reset_url_parts(&reset_url)
        else {
            return StorefrontCustomerAuthOutcome {
                value: Value::Null,
                errors: vec![storefront_not_found_error(&field.response_key)],
            };
        };
        self.storefront_reset_customer_with_token(field, &customer_id, &token, &password, true)
    }

    fn storefront_reset_customer_with_token(
        &mut self,
        field: &RootFieldSelection,
        customer_id: &str,
        reset_token: &str,
        password: &str,
        include_user_errors: bool,
    ) -> StorefrontCustomerAuthOutcome {
        let Some(mut customer) = self.storefront_customer_by_id(customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: Value::Null,
                errors: vec![storefront_not_found_error(&field.response_key)],
            };
        };
        let reset_hash = storefront_token_hash(reset_token);
        let expected_hash = customer
            .get(STOREFRONT_CUSTOMER_RESET_TOKEN_HASH_FIELD)
            .and_then(Value::as_str);
        if expected_hash != Some(reset_hash.as_str()) {
            return StorefrontCustomerAuthOutcome {
                value: selected_json(
                    &storefront_customer_activation_payload(
                        Value::Null,
                        Value::Null,
                        vec![storefront_customer_user_error(
                            ["input"],
                            "Invalid reset token",
                            Some("TOKEN_INVALID"),
                        )],
                        include_user_errors,
                    ),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        }
        customer["state"] = json!("ENABLED");
        customer["updatedAt"] = json!(self.next_product_timestamp());
        customer[STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD] =
            json!(storefront_password_fingerprint(customer_id, password));
        if let Some(object) = customer.as_object_mut() {
            object.remove(STOREFRONT_CUSTOMER_RESET_TOKEN_HASH_FIELD);
            object.remove(STOREFRONT_CUSTOMER_RESET_REQUESTED_AT_FIELD);
        }
        self.store
            .staged
            .customers
            .stage(customer_id.to_string(), customer.clone());
        let token = self.issue_storefront_customer_access_token(customer_id);
        StorefrontCustomerAuthOutcome {
            value: selected_json(
                &storefront_customer_activation_payload(
                    storefront_customer_json(&customer),
                    token,
                    Vec::new(),
                    include_user_errors,
                ),
                &field.selection,
            ),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_access_token_create_with_multipass(
        &self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let payload = storefront_customer_token_payload(
            Value::Null,
            vec![storefront_customer_user_error(
                ["multipassToken"],
                "Invalid Multipass request",
                Some("INVALID_MULTIPASS_REQUEST"),
            )],
        );
        StorefrontCustomerAuthOutcome {
            value: selected_json(&payload, &field.selection),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let token =
            resolved_string_field(&field.arguments, "customerAccessToken").unwrap_or_default();
        let input = resolved_object_field(&field.arguments, "customer").unwrap_or_default();
        let Some(customer_id) = self.storefront_customer_id_for_access_token(&token) else {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_update_payload(
                    None,
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        let Some(mut customer) = self.storefront_customer_by_id(&customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_update_payload(
                    None,
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };

        let mut errors = Vec::new();
        if input.contains_key("email") {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_update_payload(
                    None,
                    Value::Null,
                    vec![storefront_customer_user_error(
                        ["customer", "email"],
                        "CustomerUpdate access denied",
                        Some("INVALID"),
                    )],
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        }
        if input.contains_key("password")
            && resolved_string_field(&input, "password")
                .unwrap_or_default()
                .is_empty()
        {
            errors.push(storefront_customer_user_error(
                ["customer", "password"],
                "Password can't be blank",
                Some("BLANK"),
            ));
        }
        for (field_name, message, code) in [
            (
                "firstName",
                "First name cannot contain HTML tags",
                "CONTAINS_HTML_TAGS",
            ),
            (
                "lastName",
                "Last name cannot contain HTML tags",
                "CONTAINS_HTML_TAGS",
            ),
        ] {
            if resolved_string_field(&input, field_name)
                .is_some_and(|value| storefront_customer_contains_html_tag(&value))
            {
                errors.push(storefront_customer_user_error(
                    ["customer", field_name],
                    message,
                    Some(code),
                ));
            }
        }
        if !errors.is_empty() {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_update_payload(
                    None,
                    Value::Null,
                    errors,
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        }

        let old_email = customer
            .get("email")
            .and_then(Value::as_str)
            .map(storefront_customer_email_key);
        for string_field in ["firstName", "lastName", "email"] {
            if input.contains_key(string_field) {
                let value = resolved_string_field(&input, string_field)
                    .filter(|value| !value.is_empty())
                    .map(Value::String)
                    .unwrap_or(Value::Null);
                customer[string_field] = value;
            }
        }
        if input.contains_key("phone") {
            let phone = resolved_string_field(&input, "phone")
                .filter(|value| !value.is_empty())
                .map(Value::String)
                .unwrap_or(Value::Null);
            customer["phone"] = phone.clone();
            customer["defaultPhoneNumber"] = if phone.is_null() {
                Value::Null
            } else {
                json!({ "phoneNumber": phone })
            };
        }
        if let Some(accepts_marketing) = resolved_bool_field(&input, "acceptsMarketing") {
            customer["acceptsMarketing"] = json!(accepts_marketing);
            customer["emailMarketingConsent"] = json!({
                "marketingState": if accepts_marketing { "SUBSCRIBED" } else { "NOT_SUBSCRIBED" },
                "marketingOptInLevel": Value::Null,
                "consentUpdatedAt": self.next_product_timestamp()
            });
        }
        let first_name = customer.get("firstName").and_then(Value::as_str);
        let last_name = customer.get("lastName").and_then(Value::as_str);
        let email = customer.get("email").and_then(Value::as_str);
        customer["displayName"] = json!(storefront_customer_display_name(
            first_name, last_name, email
        ));
        customer["updatedAt"] = json!(self.next_product_timestamp());

        let mut new_access_token = Value::Null;
        if let Some(password) = resolved_string_field(&input, "password") {
            customer[STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD] =
                json!(storefront_password_fingerprint(&customer_id, &password));
            self.revoke_storefront_customer_access_tokens_for_customer(&customer_id);
            new_access_token = self.issue_storefront_customer_access_token(&customer_id);
        }

        if let Some(old_email) = old_email {
            self.store
                .staged
                .storefront_customer_email_index
                .remove(&old_email);
        }
        if let Some(email) = customer.get("email").and_then(Value::as_str) {
            self.store
                .staged
                .storefront_customer_email_index
                .insert(storefront_customer_email_key(email), customer_id.clone());
        }
        self.store
            .staged
            .customers
            .stage(customer_id.clone(), customer.clone());

        StorefrontCustomerAuthOutcome {
            value: self.storefront_customer_update_payload(
                Some((&customer_id, &customer)),
                new_access_token,
                Vec::new(),
                &field.selection,
            ),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_address_create(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let token =
            resolved_string_field(&field.arguments, "customerAccessToken").unwrap_or_default();
        let address_input = resolved_object_field(&field.arguments, "address").unwrap_or_default();
        let Some(customer_id) = self.storefront_customer_id_for_access_token(&token) else {
            return StorefrontCustomerAuthOutcome {
                value: Value::Null,
                errors: vec![storefront_access_denied_error(&field.response_key)],
            };
        };
        let Some(customer) = self.storefront_customer_by_id(&customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload_selected(
                    "customerAddress",
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        let new_id = self.next_proxy_synthetic_gid("MailingAddress");
        let existing_nodes = customer_address_nodes(&customer);
        let (node, errors) = customer_address_input_node(
            &address_input,
            None,
            customer.get("firstName").and_then(Value::as_str),
            customer.get("lastName").and_then(Value::as_str),
            &new_id,
        );
        if !errors.is_empty() {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload_selected(
                    "customerAddress",
                    Value::Null,
                    storefront_customer_user_errors_with_codes(errors),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        }
        let mut node = node.unwrap_or(Value::Null);
        preserve_storefront_address_phone(&mut node, &address_input);
        let new_key = customer_address_dedup_key(&node);
        if existing_nodes
            .iter()
            .any(|existing| customer_address_dedup_key(existing) == new_key)
        {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload_selected(
                    "customerAddress",
                    Value::Null,
                    vec![storefront_customer_user_error(
                        ["address"],
                        "Address already exists",
                        None,
                    )],
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        }
        let mut nodes = existing_nodes;
        let was_empty = nodes.is_empty();
        nodes.push(node.clone());
        let default_id = if was_empty {
            Some(new_id.as_str())
        } else {
            storefront_customer_default_address_id(&customer)
        };
        let updated_at = self.next_product_timestamp();
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id);
            customer["updatedAt"] = json!(updated_at);
        }
        StorefrontCustomerAuthOutcome {
            value: storefront_customer_address_payload_selected(
                "customerAddress",
                node,
                Vec::new(),
                &field.selection,
            ),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_address_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let token =
            resolved_string_field(&field.arguments, "customerAccessToken").unwrap_or_default();
        let address_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let address_input = resolved_object_field(&field.arguments, "address").unwrap_or_default();
        let Some(customer_id) = self.storefront_customer_id_for_access_token(&token) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload_selected(
                    "customerAddress",
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        let Some(customer) = self.storefront_customer_by_id(&customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload_selected(
                    "customerAddress",
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        let existing_nodes = customer_address_nodes(&customer);
        let Some(index) = storefront_customer_address_node_index(&existing_nodes, &address_id)
        else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload_selected(
                    "customerAddress",
                    Value::Null,
                    vec![storefront_customer_user_error(
                        ["id"],
                        "Address does not exist",
                        Some("NOT_FOUND"),
                    )],
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        let (node, errors) = customer_address_input_node(
            &address_input,
            Some(&existing_nodes[index]),
            customer.get("firstName").and_then(Value::as_str),
            customer.get("lastName").and_then(Value::as_str),
            &address_id,
        );
        if !errors.is_empty() {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_payload_selected(
                    "customerAddress",
                    Value::Null,
                    storefront_customer_user_errors_with_codes(errors),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        }
        let mut node = node.unwrap_or(Value::Null);
        preserve_storefront_address_phone(&mut node, &address_input);
        let mut nodes = existing_nodes;
        nodes[index] = node.clone();
        let default_id = storefront_customer_default_address_id(&customer);
        let updated_at = self.next_product_timestamp();
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id);
            customer["updatedAt"] = json!(updated_at);
        }
        StorefrontCustomerAuthOutcome {
            value: storefront_customer_address_payload_selected(
                "customerAddress",
                node,
                Vec::new(),
                &field.selection,
            ),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_address_delete(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let token =
            resolved_string_field(&field.arguments, "customerAccessToken").unwrap_or_default();
        let address_id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        let Some(customer_id) = self.storefront_customer_id_for_access_token(&token) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_delete_payload_selected(
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        let Some(customer) = self.storefront_customer_by_id(&customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_delete_payload_selected(
                    Value::Null,
                    storefront_invalid_customer_access_token_errors(),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        let mut nodes = customer_address_nodes(&customer);
        let Some(index) = storefront_customer_address_node_index(&nodes, &address_id) else {
            return StorefrontCustomerAuthOutcome {
                value: storefront_customer_address_delete_payload_selected(
                    Value::Null,
                    vec![storefront_customer_user_error(
                        ["id"],
                        "Address does not exist",
                        Some("NOT_FOUND"),
                    )],
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        let was_default =
            storefront_customer_default_address_id(&customer) == Some(address_id.as_str());
        nodes.remove(index);
        let default_id = if was_default {
            nodes
                .first()
                .and_then(|node: &Value| node.get("id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        } else {
            storefront_customer_default_address_id(&customer).map(str::to_string)
        };
        let updated_at = self.next_product_timestamp();
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, default_id.as_deref());
            customer["updatedAt"] = json!(updated_at);
        }
        StorefrontCustomerAuthOutcome {
            value: storefront_customer_address_delete_payload_selected(
                json!(address_id),
                Vec::new(),
                &field.selection,
            ),
            errors: Vec::new(),
        }
    }

    fn storefront_customer_default_address_update(
        &mut self,
        field: &RootFieldSelection,
    ) -> StorefrontCustomerAuthOutcome {
        let token =
            resolved_string_field(&field.arguments, "customerAccessToken").unwrap_or_default();
        let address_id = resolved_string_field(&field.arguments, "addressId").unwrap_or_default();
        let Some(customer_id) = self.storefront_customer_id_for_access_token(&token) else {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_default_address_payload(
                    None,
                    storefront_invalid_customer_access_token_errors(),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        let Some(customer) = self.storefront_customer_by_id(&customer_id) else {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_default_address_payload(
                    None,
                    storefront_invalid_customer_access_token_errors(),
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        };
        let nodes = customer_address_nodes(&customer);
        if storefront_customer_address_node_index(&nodes, &address_id).is_none() {
            return StorefrontCustomerAuthOutcome {
                value: self.storefront_customer_default_address_payload(
                    Some((&customer_id, &customer)),
                    vec![storefront_customer_user_error(
                        ["addressId"],
                        "Address does not exist",
                        Some("NOT_FOUND"),
                    )],
                    &field.selection,
                ),
                errors: Vec::new(),
            };
        }
        let updated_at = self.next_product_timestamp();
        if let Some(customer) = self.store.staged.customers.get_mut(&customer_id) {
            customer_rebuild_addresses(customer, nodes, Some(address_id.as_str()));
            customer["updatedAt"] = json!(updated_at);
        }
        let customer = self
            .storefront_customer_by_id(&customer_id)
            .unwrap_or(Value::Null);
        StorefrontCustomerAuthOutcome {
            value: self.storefront_customer_default_address_payload(
                Some((&customer_id, &customer)),
                Vec::new(),
                &field.selection,
            ),
            errors: Vec::new(),
        }
    }

    fn issue_storefront_customer_access_token(&mut self, customer_id: &str) -> Value {
        let sequence = self.store.staged.next_storefront_customer_access_token_id;
        self.store.staged.next_storefront_customer_access_token_id += 1;
        let issued_at = self.current_time();
        let expires_at = issued_at + time::Duration::days(42);
        let expires_at = storefront_format_timestamp(expires_at);
        let token = storefront_access_token_value(customer_id, sequence, &expires_at);
        let token_hash = storefront_token_hash(&token);
        let token_id = format!("gid://shopify/CustomerAccessToken/{sequence}");
        self.store.staged.storefront_customer_access_tokens.insert(
            token_hash,
            json!({
                "id": token_id,
                "customerId": customer_id,
                "expiresAt": expires_at,
                "revoked": false
            }),
        );
        json!({
            "accessToken": token,
            "expiresAt": expires_at
        })
    }

    fn next_storefront_customer_reset_token(&mut self, customer_id: &str) -> String {
        let sequence = self.store.staged.next_storefront_customer_reset_token_id;
        self.store.staged.next_storefront_customer_reset_token_id += 1;
        format!("sdp-reset-{}-{sequence}", resource_id_tail(customer_id))
    }

    fn storefront_access_token_is_active(&self, token_hash: &str) -> bool {
        let Some(record) = self
            .store
            .staged
            .storefront_customer_access_tokens
            .get(token_hash)
        else {
            return false;
        };
        if record["revoked"].as_bool().unwrap_or(false) {
            return false;
        }
        let Some(expires_at) = record["expiresAt"].as_str() else {
            return false;
        };
        storefront_timestamp_is_future(expires_at, self.current_time())
    }

    pub(in crate::proxy) fn storefront_customer_id_for_access_token(
        &self,
        token: &str,
    ) -> Option<String> {
        let token_hash = storefront_token_hash(token);
        if !self.storefront_access_token_is_active(&token_hash) {
            return None;
        }
        self.store
            .staged
            .storefront_customer_access_tokens
            .get(&token_hash)?
            .get("customerId")?
            .as_str()
            .map(str::to_string)
    }

    pub(in crate::proxy) fn storefront_customer_by_id(&self, customer_id: &str) -> Option<Value> {
        if self.store.staged.customers.is_tombstoned(customer_id) {
            return None;
        }
        self.store.staged.customers.get(customer_id).cloned()
    }

    fn revoke_storefront_customer_access_tokens_for_customer(&mut self, customer_id: &str) {
        for record in self
            .store
            .staged
            .storefront_customer_access_tokens
            .values_mut()
        {
            if record.get("customerId").and_then(Value::as_str) == Some(customer_id) {
                record["revoked"] = json!(true);
            }
        }
    }

    fn storefront_customer_selected_json(
        &self,
        customer_id: &str,
        customer: &Value,
        selection: &[SelectedField],
    ) -> Value {
        let base = storefront_customer_json(customer);
        selected_payload_json(selection, |field| match field.name.as_str() {
            "__typename" => Some(json!("Customer")),
            "defaultAddress" => {
                let address = customer
                    .get("defaultAddress")
                    .cloned()
                    .unwrap_or(Value::Null);
                Some(storefront_mailing_address_selected_json(
                    &address,
                    &field.selection,
                ))
            }
            "addresses" => Some(storefront_customer_addresses_connection(
                customer,
                &field.arguments,
                &field.selection,
            )),
            "orders" => Some(self.storefront_customer_orders_connection(
                customer_id,
                customer,
                &field.arguments,
                &field.selection,
            )),
            _ => selected_json(&base, std::slice::from_ref(field))
                .as_object()
                .and_then(|object| object.get(&field.response_key).cloned()),
        })
    }

    fn storefront_customer_update_payload(
        &self,
        customer: Option<(&str, &Value)>,
        customer_access_token: Value,
        customer_user_errors: Vec<Value>,
        selection: &[SelectedField],
    ) -> Value {
        let customer_user_errors = storefront_customer_user_errors_with_codes(customer_user_errors);
        selected_payload_json(selection, |field| match field.name.as_str() {
            "customer" => Some(
                customer
                    .map(|(customer_id, customer)| {
                        self.storefront_customer_selected_json(
                            customer_id,
                            customer,
                            &field.selection,
                        )
                    })
                    .unwrap_or(Value::Null),
            ),
            "customerAccessToken" => Some(selected_json(&customer_access_token, &field.selection)),
            "customerUserErrors" => Some(selected_user_errors(
                &customer_user_errors,
                &field.selection,
            )),
            "userErrors" => {
                let errors = storefront_user_errors_without_code(&customer_user_errors);
                Some(selected_user_errors(
                    errors.as_array().map(Vec::as_slice).unwrap_or(&[]),
                    &field.selection,
                ))
            }
            _ => None,
        })
    }

    fn storefront_customer_default_address_payload(
        &self,
        customer: Option<(&str, &Value)>,
        customer_user_errors: Vec<Value>,
        selection: &[SelectedField],
    ) -> Value {
        let customer_user_errors = storefront_customer_user_errors_with_codes(customer_user_errors);
        selected_payload_json(selection, |field| match field.name.as_str() {
            "customer" => Some(
                customer
                    .map(|(customer_id, customer)| {
                        self.storefront_customer_selected_json(
                            customer_id,
                            customer,
                            &field.selection,
                        )
                    })
                    .unwrap_or(Value::Null),
            ),
            "customerUserErrors" => Some(selected_user_errors(
                &customer_user_errors,
                &field.selection,
            )),
            "userErrors" => {
                let errors = storefront_user_errors_without_code(&customer_user_errors);
                Some(selected_user_errors(
                    errors.as_array().map(Vec::as_slice).unwrap_or(&[]),
                    &field.selection,
                ))
            }
            _ => None,
        })
    }

    fn storefront_customer_orders_connection(
        &self,
        customer_id: &str,
        customer: &Value,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let orders = self
            .store
            .staged
            .customer_orders
            .get(customer_id)
            .cloned()
            .unwrap_or_else(|| connection_nodes(&customer["orders"]));
        selected_typed_connection_with_args(
            &orders,
            arguments,
            selection,
            |order, order_selection| selected_json(&storefront_order_json(order), order_selection),
            storefront_order_cursor,
        )
    }

    fn storefront_customer_id_by_email(&self, normalized_email: &str) -> Option<String> {
        if let Some(customer_id) = self
            .store
            .staged
            .storefront_customer_email_index
            .get(normalized_email)
            .filter(|customer_id| self.storefront_customer_by_id(customer_id).is_some())
        {
            return Some(customer_id.clone());
        }
        self.store
            .staged
            .customers
            .iter()
            .find_map(|(customer_id, customer)| {
                let email = customer.get("email").and_then(Value::as_str)?;
                (storefront_customer_email_key(email) == normalized_email)
                    .then(|| customer_id.clone())
            })
    }

    fn storefront_customer_activation_url_parts(&self, url: &str) -> Option<(String, String)> {
        let token = url.rsplit('/').next()?.to_string();
        if token.is_empty() {
            return None;
        }
        let customer_id = self
            .store
            .staged
            .customers
            .iter()
            .find_map(|(id, customer)| {
                let deterministic = storefront_customer_activation_token_for_id(id);
                let stored = customer
                    .get(STOREFRONT_CUSTOMER_ACTIVATION_TOKEN_FIELD)
                    .and_then(Value::as_str);
                (token == deterministic || stored == Some(token.as_str())).then(|| id.clone())
            })?;
        Some((customer_id, token))
    }

    fn storefront_customer_reset_url_parts(&self, url: &str) -> Option<(String, String)> {
        let token = url.rsplit('/').next()?.to_string();
        if token.is_empty() {
            return None;
        }
        let token_hash = storefront_token_hash(&token);
        let customer_id = self
            .store
            .staged
            .customers
            .iter()
            .find_map(|(id, customer)| {
                (customer
                    .get(STOREFRONT_CUSTOMER_RESET_TOKEN_HASH_FIELD)
                    .and_then(Value::as_str)
                    == Some(token_hash.as_str()))
                .then(|| id.clone())
            })?;
        Some((customer_id, token))
    }

    pub(in crate::proxy) fn record_storefront_customer_auth_log_entry(
        &mut self,
        request: &Request,
        query: &str,
        variables: &BTreeMap<String, ResolvedValue>,
        fields: &[RootFieldSelection],
        details: StorefrontCustomerAuthLogDetails<'_>,
    ) {
        let operation = parse_operation_with_variables(query, variables);
        let id = format!("log-{}", self.log_entries.len() + 1);
        let root_fields = fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<Vec<_>>();
        let primary_root_field = root_fields.first().cloned().unwrap_or_default();
        let operation_type = operation
            .as_ref()
            .map(|operation| operation.operation_type.keyword())
            .unwrap_or("unknown");
        self.log_entries.push(json!({
            "id": id,
            "operationName": Value::Null,
            "apiSurface": "storefront",
            "status": details.status,
            "path": request.path,
            "query": "<redacted:storefront-customer-auth-query>",
            "variables": storefront_redacted_variables_json(variables),
            "rawBody": "<redacted:storefront-customer-auth-request>",
            "interpreted": {
                "operationType": operation_type,
                "rootFields": root_fields,
                "primaryRootField": primary_root_field,
                "capability": {
                    "domain": "storefront",
                    "execution": details.execution
                }
            },
            "notes": details.notes
        }));
    }

    pub(crate) fn resolve_storefront_graphql(
        &mut self,
        execution: RootResolverContext<'_>,
    ) -> Response {
        let RootResolverContext {
            request,
            query,
            variables,
            operation,
            root_name,
            mode,
        } = execution;
        if storefront_graphql_version(&request.path) != Some(STOREFRONT_FIRST_SLICE_VERSION)
            || self.config.read_mode == ReadMode::Live
        {
            return Self::unimplemented_resolver_response(mode, root_name);
        }
        let Some(field) = self.execution_root_field(query, variables, root_name) else {
            return json_error(400, "Could not parse Storefront GraphQL root field");
        };
        if let Some(error) = storefront_discovery_argument_error(&field) {
            let mut data = serde_json::Map::new();
            data.insert(field.response_key.clone(), Value::Null);
            return ok_json(json!({
                "data": Value::Object(data),
                "errors": [{
                    "message": error.0,
                    "path": [field.response_key],
                    "extensions": error.1
                }]
            }));
        }
        match (operation.operation_type, mode) {
            (OperationType::Query, LocalResolverMode::OverlayRead) if root_name == "cart" => {
                let outcome = self.storefront_cart_query_root(&field);
                let mut data = serde_json::Map::new();
                data.insert(field.response_key.clone(), outcome.value);
                let mut body = json!({ "data": Value::Object(data) });
                if !outcome.errors.is_empty() {
                    body["errors"] = Value::Array(outcome.errors);
                }
                return ok_json(body);
            }
            (OperationType::Query, LocalResolverMode::OverlayRead) if root_name == "customer" => {
                let outcome = self.storefront_customer_query_root(&field);
                let mut data = serde_json::Map::new();
                data.insert(field.response_key.clone(), outcome.value);
                let mut body = json!({ "data": Value::Object(data) });
                if !outcome.errors.is_empty() {
                    body["errors"] = Value::Array(outcome.errors);
                }
                return ok_json(body);
            }
            (OperationType::Mutation, LocalResolverMode::StageLocally)
                if STOREFRONT_CART_MUTATION_ROOTS.contains(&root_name) =>
            {
                let outcome = self.storefront_cart_mutation_root(&field);
                let mut data = serde_json::Map::new();
                data.insert(field.response_key.clone(), outcome.value);
                let mut body = json!({ "data": Value::Object(data) });
                if !outcome.errors.is_empty() {
                    body["errors"] = Value::Array(outcome.errors);
                }
                return ok_json(body);
            }
            (OperationType::Mutation, LocalResolverMode::StageLocally)
                if STOREFRONT_CUSTOMER_AUTH_MUTATION_ROOTS.contains(&root_name) =>
            {
                let outcome = self.storefront_customer_mutation_root(&field);
                let mut data = serde_json::Map::new();
                data.insert(field.response_key.clone(), outcome.value);
                let mut body = json!({ "data": Value::Object(data) });
                if !outcome.errors.is_empty() {
                    body["errors"] = Value::Array(outcome.errors);
                }
                return ok_json(body);
            }
            (OperationType::Query, LocalResolverMode::OverlayRead) => {}
            _ => return Self::unimplemented_resolver_response(mode, root_name),
        }
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.storefront_fields_include_catalog(std::slice::from_ref(&field))
            && !self.storefront_catalog_is_locally_ready()
        {
            return Self::unimplemented_resolver_response(mode, root_name);
        }

        let context = storefront_request_context(query, variables);
        if self.config.read_mode == ReadMode::LiveHybrid
            && STOREFRONT_COLLECTION_ROOTS.contains(&field.name.as_str())
            && self.storefront_collection_field_needs_hydration(&field)
        {
            self.hydrate_storefront_collections(request);
        }
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.storefront_first_slice_needs_hydration(std::slice::from_ref(&field), &context)
        {
            self.hydrate_storefront_first_slice(request, &context);
        }
        if self.config.read_mode == ReadMode::LiveHybrid {
            self.hydrate_storefront_taxonomy_for_fields(request, std::slice::from_ref(&field));
            self.hydrate_storefront_menus_for_fields(request, std::slice::from_ref(&field));
        }

        ok_json(json!({
            "data": self.storefront_local_query_data(&[field], &context)
        }))
    }

    pub(in crate::proxy) fn storefront_fields_are_local(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        if self.config.read_mode == ReadMode::LiveHybrid
            && self.storefront_fields_include_catalog(fields)
            && !self.storefront_catalog_is_locally_ready()
        {
            return false;
        }
        fields
            .iter()
            .all(|field| self.storefront_field_is_local(field))
    }

    fn storefront_field_is_local(&self, field: &RootFieldSelection) -> bool {
        let capability = self.registry.resolve_for_surface(
            ApiSurface::Storefront,
            OperationType::Query,
            &field.name,
        );
        capability.domain == CapabilityDomain::Storefront
            && self.storefront_root_is_promoted(&field.name)
            && self.storefront_root_has_local_backing(field)
    }

    fn storefront_custom_data_field_has_local_effect(&self, field: &RootFieldSelection) -> bool {
        match field.name.as_str() {
            "metaobject" => self.has_local_metaobject_state(),
            "metaobjects" => {
                let meta_type = resolved_string_field(&field.arguments, "type").unwrap_or_default();
                meta_type.is_empty()
                    || self.metaobject_definition_by_type(&meta_type).is_some()
                    || self.store.staged.metaobjects.values().any(|record| {
                        record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                    })
            }
            _ => false,
        }
    }

    pub(in crate::proxy) fn storefront_mutation_fields_are_local(
        &self,
        fields: &[RootFieldSelection],
    ) -> bool {
        fields.iter().all(|field| {
            (STOREFRONT_CUSTOMER_AUTH_MUTATION_ROOTS.contains(&field.name.as_str())
                || STOREFRONT_CART_MUTATION_ROOTS.contains(&field.name.as_str()))
                && self
                    .registry
                    .resolve_for_surface(
                        ApiSurface::Storefront,
                        OperationType::Mutation,
                        &field.name,
                    )
                    .execution
                    == CapabilityExecution::StageLocally
        })
    }

    fn storefront_root_is_promoted(&self, root: &str) -> bool {
        root == "cart"
            || root == "customer"
            || STOREFRONT_FIRST_SLICE_ROOTS.contains(&root)
            || STOREFRONT_COLLECTION_ROOTS.contains(&root)
            || STOREFRONT_LOCAL_CONTENT_ROOTS.contains(&root)
            || STOREFRONT_CUSTOM_DATA_ROOTS.contains(&root)
            || STOREFRONT_DISCOVERY_ROOTS.contains(&root)
    }

    fn storefront_root_has_local_backing(&self, field: &RootFieldSelection) -> bool {
        if self.config.read_mode == ReadMode::Snapshot
            || STOREFRONT_FIRST_SLICE_ROOTS.contains(&field.name.as_str())
        {
            return true;
        }
        match field.name.as_str() {
            "cart" => true,
            "customer" => true,
            root if STOREFRONT_COLLECTION_ROOTS.contains(&root) => true,
            root if STOREFRONT_CONTENT_ROOTS.contains(&root) => {
                self.has_online_store_content_state()
            }
            "sitemap" => self.has_online_store_content_state(),
            "urlRedirects" => self.has_staged_url_redirects(),
            "menu" => true,
            root if STOREFRONT_CUSTOM_DATA_ROOTS.contains(&root) => {
                self.storefront_custom_data_field_has_local_effect(field)
            }
            root if STOREFRONT_DISCOVERY_ROOTS.contains(&root) => {
                self.has_storefront_discovery_state()
            }
            _ => false,
        }
    }

    fn has_storefront_discovery_state(&self) -> bool {
        self.store.has_product_state()
            || !self.store.staged.collections.is_empty()
            || self.has_online_store_content_state()
            || !self.store.staged.metaobjects.is_empty()
            || !self
                .store
                .base
                .storefront_locations
                .ordered_values()
                .is_empty()
            || !self.store.base.storefront_menus.ordered_values().is_empty()
    }

    fn storefront_fields_include_catalog(&self, fields: &[RootFieldSelection]) -> bool {
        fields.iter().any(|field| {
            matches!(
                field.name.as_str(),
                "product" | "productByHandle" | "productRecommendations" | "products"
            )
        })
    }

    fn storefront_catalog_is_locally_ready(&self) -> bool {
        self.store.has_product_state()
            && (self.store.staged.current_channel_publication_resolved
                || self.store.has_known_publication_catalog())
    }

    fn storefront_first_slice_needs_hydration(
        &self,
        fields: &[RootFieldSelection],
        context: &StorefrontRequestContext,
    ) -> bool {
        fields.iter().any(|field| match field.name.as_str() {
            "shop" => self.storefront_shop_needs_hydration(&field.selection),
            "localization" => !self.storefront_localization_is_observed(context),
            "locations" => self.storefront_location_records().is_empty(),
            "paymentSettings" => self
                .storefront_payment_settings_source()
                .is_none_or(|settings| !settings.is_object()),
            "publicApiVersions" => self.store.base.storefront_public_api_versions.is_empty(),
            "product" | "productByHandle" | "products" => false,
            _ => false,
        })
    }

    fn storefront_shop_needs_hydration(&self, selections: &[SelectedField]) -> bool {
        if self.store.base.storefront_shop.is_object() {
            return false;
        }
        let admin_shop = if self.store.base.shop.is_object() {
            self.store.effective_shop()
        } else {
            Value::Null
        };
        if !admin_shop.is_object() {
            return !self.storefront_shop_selection_uses_only_local_metafields(selections);
        }
        selections
            .iter()
            .filter(|selection| selection_applies_to_type(selection, "Shop"))
            .any(|selection| !self.storefront_shop_field_has_admin_source(&admin_shop, selection))
    }

    fn storefront_shop_selection_uses_only_local_metafields(
        &self,
        selections: &[SelectedField],
    ) -> bool {
        let mut has_metafield_selection = false;
        for selection in selections
            .iter()
            .filter(|selection| selection_applies_to_type(selection, "Shop"))
        {
            match selection.name.as_str() {
                "__typename" => {}
                "metafield" | "metafields" => {
                    if !self.storefront_has_local_shop_metafield_state() {
                        return false;
                    }
                    has_metafield_selection = true;
                }
                _ => return false,
            }
        }
        has_metafield_selection
    }

    fn storefront_has_local_shop_metafield_state(&self) -> bool {
        self.storefront_shop_owner_id().is_some_and(|owner_id| {
            self.store
                .staged
                .owner_metafields
                .get(&owner_id)
                .is_some_and(|records| !records.is_empty())
                || self
                    .store
                    .staged
                    .deleted_owner_metafields
                    .iter()
                    .any(|(deleted_owner_id, _, _)| deleted_owner_id == &owner_id)
        })
    }

    fn storefront_shop_field_has_admin_source(
        &self,
        shop: &Value,
        selection: &SelectedField,
    ) -> bool {
        match selection.name.as_str() {
            "__typename" => true,
            "id" | "name" => shop.get(&selection.name).is_some(),
            "primaryDomain" => shop.get("primaryDomain").is_some(),
            "paymentSettings" => self
                .storefront_payment_settings_source()
                .is_some_and(|settings| settings.is_object()),
            "privacyPolicy" => self.store.shop_policy_by_type("PRIVACY_POLICY").is_some(),
            "refundPolicy" => self.store.shop_policy_by_type("REFUND_POLICY").is_some(),
            "shippingPolicy" => self.store.shop_policy_by_type("SHIPPING_POLICY").is_some(),
            "termsOfService" => self.store.shop_policy_by_type("TERMS_OF_SERVICE").is_some(),
            "termsOfSale" => self.store.shop_policy_by_type("TERMS_OF_SALE").is_some(),
            "legalNotice" => self.store.shop_policy_by_type("LEGAL_NOTICE").is_some(),
            "contactInformation" => self
                .store
                .shop_policy_by_type("CONTACT_INFORMATION")
                .is_some(),
            "moneyFormat" => self.store.shop_money_format().is_some(),
            "metafield" | "metafields" => self.storefront_has_local_shop_metafield_state(),
            _ => false,
        }
    }

    fn hydrate_storefront_first_slice(
        &mut self,
        request: &Request,
        context: &StorefrontRequestContext,
    ) {
        let (query, variables) = storefront_first_slice_hydrate_body(context);
        let response = self.storefront_upstream_post(
            request,
            json!({
                "query": query,
                "variables": variables
            }),
        );
        if (200..300).contains(&response.status) {
            self.hydrate_storefront_first_slice_from_data(&response.body["data"], context);
        }
    }

    fn hydrate_storefront_taxonomy_for_fields(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) {
        let needs_tags = fields.iter().any(|field| field.name == "productTags")
            && !self.store.base.storefront_product_tags.is_object();
        let needs_types = fields.iter().any(|field| field.name == "productTypes")
            && !self.store.base.storefront_product_types.is_object();
        if !needs_tags && !needs_types {
            return;
        }
        let response = self.storefront_upstream_post(
            request,
            json!({
                "query": STOREFRONT_ENRICHMENT_TAXONOMY_HYDRATE_QUERY,
                "variables": {}
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        if let Some(connection) = response
            .body
            .pointer("/data/productTags")
            .filter(|value| value.is_object())
        {
            self.store.base.storefront_product_tags = connection.clone();
        }
        if let Some(connection) = response
            .body
            .pointer("/data/productTypes")
            .filter(|value| value.is_object())
        {
            self.store.base.storefront_product_types = connection.clone();
        }
    }

    fn storefront_upstream_post(&self, request: &Request, body: Value) -> Response {
        (self.storefront_upstream_transport)(Request {
            method: "POST".to_string(),
            path: request.path.clone(),
            headers: request.headers.clone(),
            body: body.to_string(),
        })
    }

    fn hydrate_storefront_first_slice_from_data(
        &mut self,
        data: &Value,
        context: &StorefrontRequestContext,
    ) {
        if let Some(shop) = data.get("shop").filter(|shop| shop.is_object()) {
            self.store.base.storefront_shop =
                shallow_merged_object(self.store.base.storefront_shop.clone(), shop.clone());
        }
        if let Some(localization) = data.get("localization").filter(|value| value.is_object()) {
            self.store
                .base
                .storefront_localizations
                .insert(context.key(), localization.clone());
        }
        if let Some(settings) = data
            .get("paymentSettings")
            .filter(|value| value.is_object())
        {
            self.store.base.storefront_payment_settings = shallow_merged_object(
                self.store.base.storefront_payment_settings.clone(),
                settings.clone(),
            );
        } else if let Some(settings) = data
            .pointer("/shop/paymentSettings")
            .filter(|value| value.is_object())
        {
            self.store.base.storefront_payment_settings = shallow_merged_object(
                self.store.base.storefront_payment_settings.clone(),
                settings.clone(),
            );
        }
        if let Some(versions) = data.get("publicApiVersions").and_then(Value::as_array) {
            self.store.base.storefront_public_api_versions = versions.clone();
        }
        self.hydrate_storefront_locations_from_connection(data.get("locations"));
    }

    fn hydrate_storefront_locations_from_connection(&mut self, connection: Option<&Value>) {
        let Some(connection) = connection.filter(|value| value.is_object()) else {
            return;
        };
        let mut cursor_by_id = BTreeMap::new();
        if let Some(edges) = connection.get("edges").and_then(Value::as_array) {
            for edge in edges {
                let Some(node) = edge.get("node").filter(|node| node.is_object()) else {
                    continue;
                };
                if let (Some(id), Some(cursor)) = (
                    node.get("id").and_then(Value::as_str),
                    edge.get("cursor").and_then(Value::as_str),
                ) {
                    cursor_by_id.insert(id.to_string(), cursor.to_string());
                }
            }
        }
        for node in connection_nodes(connection) {
            let Some(id) = node.get("id").and_then(Value::as_str).map(str::to_string) else {
                continue;
            };
            self.store
                .base
                .storefront_locations
                .insert(id.clone(), node);
            if let Some(cursor) = cursor_by_id.get(&id) {
                self.store
                    .base
                    .storefront_location_cursors
                    .insert(id, cursor.clone());
            }
        }
    }

    fn hydrate_storefront_menus_for_fields(
        &mut self,
        request: &Request,
        fields: &[RootFieldSelection],
    ) {
        for field in fields.iter().filter(|field| field.name == "menu") {
            let Some(handle) = resolved_string_field(&field.arguments, "handle") else {
                continue;
            };
            if self.storefront_menu_by_handle(&handle).is_some() {
                continue;
            }
            self.hydrate_storefront_menu(request, &handle);
        }
    }

    fn hydrate_storefront_menu(&mut self, request: &Request, handle: &str) {
        let response = self.storefront_upstream_post(
            request,
            json!({
                "query": STOREFRONT_MENU_HYDRATE_QUERY,
                "variables": { "handle": handle }
            }),
        );
        if !(200..300).contains(&response.status) {
            return;
        }
        let Some(menu) = response
            .body
            .pointer("/data/menu")
            .filter(|menu| menu.is_object())
            .cloned()
        else {
            return;
        };
        let Some(id) = menu.get("id").and_then(Value::as_str).map(str::to_string) else {
            return;
        };
        self.store.base.storefront_menus.insert(id, menu);
    }

    pub(in crate::proxy) fn storefront_local_query_data(
        &self,
        fields: &[RootFieldSelection],
        context: &StorefrontRequestContext,
    ) -> Value {
        root_payload_json(fields, |field| {
            Some(match field.name.as_str() {
                "shop" => self.storefront_shop_json(&field.selection),
                "cart" => self.storefront_cart_query_root(field).value,
                "localization" => self.storefront_localization_json(context, &field.selection),
                "locations" => {
                    self.storefront_locations_connection_json(&field.arguments, &field.selection)
                }
                "paymentSettings" => self.storefront_payment_settings_json(&field.selection),
                "publicApiVersions" => Value::Array(
                    self.store
                        .base
                        .storefront_public_api_versions
                        .iter()
                        .map(|version| selected_json(version, &field.selection))
                        .collect(),
                ),
                "article" => self.storefront_article_root(field),
                "articles" => self.storefront_content_connection(
                    StorefrontContentKind::Article,
                    self.storefront_article_records(),
                    &field.arguments,
                    &field.selection,
                ),
                "blog" => self.storefront_blog_root(field),
                "blogByHandle" => self.storefront_blog_by_handle_root(field),
                "blogs" => self.storefront_content_connection(
                    StorefrontContentKind::Blog,
                    self.storefront_blog_records(),
                    &field.arguments,
                    &field.selection,
                ),
                "page" => self.storefront_page_root(field),
                "pageByHandle" => self.storefront_page_by_handle_root(field),
                "pages" => self.storefront_content_connection(
                    StorefrontContentKind::Page,
                    self.storefront_page_records(),
                    &field.arguments,
                    &field.selection,
                ),
                "menu" => self.storefront_menu_root(field),
                "sitemap" => self.storefront_sitemap_root(field),
                "urlRedirects" => self
                    .url_redirect_query_data(std::slice::from_ref(field))
                    .get(&field.response_key)
                    .cloned()
                    .unwrap_or(Value::Null),
                "product" => self.storefront_product_field_json(field, context),
                "productByHandle" => self.storefront_product_by_handle_field_json(field, context),
                "productRecommendations" => {
                    self.storefront_product_recommendations_json(field, context)
                }
                "productTags" => self.storefront_product_taxonomy_connection_json(
                    field,
                    StorefrontProductTaxonomyKind::Tag,
                ),
                "productTypes" => self.storefront_product_taxonomy_connection_json(
                    field,
                    StorefrontProductTaxonomyKind::ProductType,
                ),
                "products" => self.storefront_products_connection_json(field, context),
                "collection" => self.storefront_collection_field_json(field, context),
                "collectionByHandle" => {
                    self.storefront_collection_by_handle_field_json(field, context)
                }
                "collections" => self.storefront_collections_connection_json(field, context),
                "metaobject" => self.storefront_metaobject_root_json(field),
                "metaobjects" => self.storefront_metaobjects_connection_json(field),
                "node" => self.storefront_node_root_json(field, context),
                "nodes" => self.storefront_nodes_root_json(field, context),
                "search" => self.storefront_search_root_json(field, context),
                "predictiveSearch" => self.storefront_predictive_search_root_json(field, context),
                _ => Value::Null,
            })
        })
    }

    fn storefront_node_root_json(
        &self,
        field: &RootFieldSelection,
        context: &StorefrontRequestContext,
    ) -> Value {
        resolved_string_field(&field.arguments, "id")
            .map(|id| self.storefront_node_by_id_json(&id, context, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_nodes_root_json(
        &self,
        field: &RootFieldSelection,
        context: &StorefrontRequestContext,
    ) -> Value {
        Value::Array(
            list_string_field(&field.arguments, "ids")
                .into_iter()
                .map(|id| self.storefront_node_by_id_json(&id, context, &field.selection))
                .collect(),
        )
    }

    fn storefront_node_by_id_json(
        &self,
        id: &str,
        context: &StorefrontRequestContext,
        selections: &[SelectedField],
    ) -> Value {
        match shopify_gid_resource_type(id) {
            Some("Product") => self.storefront_visible_product_json(
                self.store.product_by_id(id),
                context,
                selections,
            ),
            Some("ProductVariant") => self
                .store
                .product_variant_by_id(id)
                .filter(|variant| {
                    self.store
                        .product_by_id(&variant.product_id)
                        .is_some_and(|product| self.storefront_product_is_visible(product))
                })
                .map(|variant| {
                    storefront_product_variant_json(
                        self,
                        variant,
                        self.store.product_by_id(&variant.product_id),
                        context,
                        None,
                        selections,
                    )
                })
                .unwrap_or(Value::Null),
            Some("Collection") => self.storefront_visible_collection_json(
                self.store.collection_by_id(id),
                context,
                selections,
            ),
            Some("Article") => self
                .storefront_content_by_id(StorefrontContentKind::Article, id)
                .map(|record| self.selected_storefront_article(&record, selections))
                .unwrap_or(Value::Null),
            Some("Blog") => self
                .storefront_content_by_id(StorefrontContentKind::Blog, id)
                .map(|record| self.selected_storefront_blog(&record, selections))
                .unwrap_or(Value::Null),
            Some("Page") => self
                .storefront_content_by_id(StorefrontContentKind::Page, id)
                .map(|record| self.selected_storefront_page(&record, selections))
                .unwrap_or(Value::Null),
            Some("Metaobject") => self
                .metaobject_by_id(id)
                .and_then(|record| self.storefront_visible_metaobject(&record))
                .map(|record| self.storefront_selected_metaobject(&record, selections))
                .unwrap_or(Value::Null),
            Some("Location") => self
                .storefront_location_records()
                .into_iter()
                .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
                .map(|record| selected_json(&record, selections))
                .unwrap_or(Value::Null),
            Some("Menu") => self
                .store
                .base
                .storefront_menus
                .ordered_values()
                .into_iter()
                .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
                .map(|record| selected_json(record, selections))
                .unwrap_or(Value::Null),
            _ => Value::Null,
        }
    }

    fn storefront_search_root_json(
        &self,
        field: &RootFieldSelection,
        context: &StorefrontRequestContext,
    ) -> Value {
        let mut items = self.storefront_search_items(&field.arguments);
        storefront_sort_search_items(self, &mut items, &field.arguments);
        let total_count = items.len();
        let filter_items = items.clone();
        let (items, page_info) =
            connection_window(&items, &field.arguments, storefront_search_item_cursor);
        let node_selection = nested_selected_fields(&field.selection, &["nodes"]);
        let edge_node_selection = nested_selected_fields(&field.selection, &["edges", "node"]);
        selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
            "nodes" => Some(Value::Array(
                items
                    .iter()
                    .map(|item| self.storefront_search_item_json(item, context, &node_selection))
                    .collect(),
            )),
            "edges" => Some(Value::Array(
                items
                    .iter()
                    .map(|item| {
                        json!({
                            "cursor": storefront_search_item_cursor(item),
                            "node": self.storefront_search_item_json(item, context, &edge_node_selection)
                        })
                    })
                    .collect(),
            )),
            "pageInfo" => Some(selected_json(&page_info, &selection.selection)),
            "totalCount" => Some(json!(total_count)),
            "productFilters" => Some(Value::Array(
                storefront_search_product_filters(self, &filter_items)
                    .iter()
                    .map(|filter| selected_json(filter, &selection.selection))
                    .collect(),
            )),
            _ => None,
        }
        })
    }

    fn storefront_search_items(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
    ) -> Vec<StorefrontSearchItem> {
        let requested_types = list_string_field(arguments, "types");
        let includes = |name: &str| {
            requested_types
                .first()
                .is_none_or(|requested_type| requested_type == name)
        };
        let query = resolved_string_field(arguments, "query").unwrap_or_default();
        let prefix =
            resolved_string_field(arguments, "prefix").unwrap_or_else(|| "NONE".to_string());
        let unavailable = resolved_string_field(arguments, "unavailableProducts")
            .unwrap_or_else(|| "LAST".to_string());
        let product_filters = resolved_object_list_field(arguments, "productFilters");
        let mut items = Vec::new();
        if includes("PRODUCT") {
            items.extend(
                self.storefront_visible_products()
                    .into_iter()
                    .filter(|product| {
                        storefront_product_matches_discovery_query(
                            self,
                            product,
                            &query,
                            &prefix,
                            &[],
                        )
                    })
                    .filter(|product| {
                        storefront_product_matches_search_filters(self, product, &product_filters)
                    })
                    .filter(|product| {
                        unavailable != "HIDE" || storefront_search_product_available(self, product)
                    })
                    .map(|product| StorefrontSearchItem::Product(Box::new(product))),
            );
        }
        if includes("ARTICLE") {
            items.extend(
                self.storefront_article_records()
                    .into_iter()
                    .filter(|record| {
                        storefront_value_matches_discovery_query(record, &query, &prefix, &[])
                    })
                    .map(StorefrontSearchItem::Article),
            );
        }
        if includes("PAGE") {
            items.extend(
                self.storefront_page_records()
                    .into_iter()
                    .filter(|record| {
                        storefront_value_matches_discovery_query(record, &query, &prefix, &[])
                    })
                    .map(StorefrontSearchItem::Page),
            );
        }
        if unavailable == "LAST" {
            items.sort_by_key(|item| match item {
                StorefrontSearchItem::Product(product) => {
                    !storefront_search_product_available(self, product)
                }
                _ => false,
            });
        }
        items
    }

    fn storefront_search_item_json(
        &self,
        item: &StorefrontSearchItem,
        context: &StorefrontRequestContext,
        selections: &[SelectedField],
    ) -> Value {
        let (mut projected, type_name, id) = match item {
            StorefrontSearchItem::Product(product) => {
                let variants = self.store.product_variants_for_product(&product.id);
                (
                    storefront_product_json(self, product, &variants, context, selections),
                    "Product",
                    product.id.as_str(),
                )
            }
            StorefrontSearchItem::Article(article) => (
                self.selected_storefront_article(article, selections),
                "Article",
                article
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            ),
            StorefrontSearchItem::Page(page) => (
                self.selected_storefront_page(page, selections),
                "Page",
                page.get("id").and_then(Value::as_str).unwrap_or_default(),
            ),
        };
        if let Some(object) = projected.as_object_mut() {
            object
                .entry("__typename".to_string())
                .or_insert_with(|| json!(type_name));
            object.entry("id".to_string()).or_insert_with(|| json!(id));
        }
        projected
    }

    fn storefront_predictive_search_root_json(
        &self,
        field: &RootFieldSelection,
        context: &StorefrontRequestContext,
    ) -> Value {
        let query = resolved_string_field(&field.arguments, "query").unwrap_or_default();
        let limit = resolved_int_field(&field.arguments, "limit")
            .unwrap_or(10)
            .clamp(1, 10) as usize;
        let limit_scope = resolved_string_field(&field.arguments, "limitScope")
            .unwrap_or_else(|| "ALL".to_string());
        let requested_types = list_string_field(&field.arguments, "types");
        let includes = |name: &str| {
            requested_types.is_empty() || requested_types.iter().any(|value| value == name)
        };
        let searchable_fields = list_string_field(&field.arguments, "searchableFields");
        let unavailable = resolved_string_field(&field.arguments, "unavailableProducts")
            .unwrap_or_else(|| "LAST".to_string());
        let mut products = if includes("PRODUCT") {
            self.storefront_visible_products()
                .into_iter()
                .filter(|product| {
                    storefront_product_matches_discovery_query(
                        self,
                        product,
                        &query,
                        "LAST",
                        &searchable_fields,
                    )
                })
                .filter(|product| {
                    unavailable != "HIDE" || storefront_search_product_available(self, product)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        products.sort_by(|left, right| {
            left.title
                .to_ascii_lowercase()
                .cmp(&right.title.to_ascii_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        if unavailable == "LAST" {
            products.sort_by_key(|product| !storefront_search_product_available(self, product));
        }
        let mut collections = if includes("COLLECTION") {
            self.store
                .staged
                .collections
                .values()
                .filter(|record| self.storefront_collection_is_visible(record))
                .filter(|record| {
                    storefront_value_matches_discovery_query(
                        record,
                        &query,
                        "LAST",
                        &searchable_fields,
                    )
                })
                .cloned()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut articles = if includes("ARTICLE") {
            self.storefront_article_records()
                .into_iter()
                .filter(|record| {
                    storefront_value_matches_discovery_query(
                        record,
                        &query,
                        "LAST",
                        &searchable_fields,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut pages = if includes("PAGE") {
            self.storefront_page_records()
                .into_iter()
                .filter(|record| {
                    storefront_value_matches_discovery_query(
                        record,
                        &query,
                        "LAST",
                        &searchable_fields,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        for records in [&mut collections, &mut articles, &mut pages] {
            records.sort_by(|left, right| {
                storefront_value_title(left)
                    .cmp(&storefront_value_title(right))
                    .then_with(|| value_id_cursor(left).cmp(&value_id_cursor(right)))
            });
        }
        if limit_scope == "ALL" {
            let mut remaining = limit;
            truncate_with_remaining(&mut products, &mut remaining);
            truncate_with_remaining(&mut collections, &mut remaining);
            truncate_with_remaining(&mut pages, &mut remaining);
            truncate_with_remaining(&mut articles, &mut remaining);
        } else {
            products.truncate(limit);
            collections.truncate(limit);
            articles.truncate(limit);
            pages.truncate(limit);
        }
        let suggestions = if includes("QUERY") {
            let suggestion_products = self.storefront_visible_products();
            let suggestion_collections = self
                .store
                .staged
                .collections
                .values()
                .filter(|record| self.storefront_collection_is_visible(record))
                .cloned()
                .collect::<Vec<_>>();
            let suggestion_articles = self.storefront_article_records();
            let suggestion_pages = self.storefront_page_records();
            storefront_query_suggestions(
                &query,
                limit,
                &suggestion_products,
                &suggestion_collections,
                &suggestion_articles,
                &suggestion_pages,
            )
        } else {
            Vec::new()
        };
        selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
                "products" => Some(Value::Array(
                    products
                        .iter()
                        .map(|product| {
                            let variants = self.store.product_variants_for_product(&product.id);
                            storefront_product_json(
                                self,
                                product,
                                &variants,
                                context,
                                &selection.selection,
                            )
                        })
                        .collect(),
                )),
                "collections" => Some(Value::Array(
                    collections
                        .iter()
                        .map(|record| {
                            self.storefront_collection_json(record, context, &selection.selection)
                        })
                        .collect(),
                )),
                "articles" => Some(Value::Array(
                    articles
                        .iter()
                        .map(|record| {
                            self.selected_storefront_article(record, &selection.selection)
                        })
                        .collect(),
                )),
                "pages" => Some(Value::Array(
                    pages
                        .iter()
                        .map(|record| self.selected_storefront_page(record, &selection.selection))
                        .collect(),
                )),
                "queries" => Some(Value::Array(
                    suggestions
                        .iter()
                        .map(|record| selected_json(record, &selection.selection))
                        .collect(),
                )),
                _ => None,
            }
        })
    }

    fn storefront_collection_field_needs_hydration(&self, _field: &RootFieldSelection) -> bool {
        self.store.staged.collections.is_empty()
    }

    fn hydrate_storefront_collections(&mut self, request: &Request) {
        let response = (self.storefront_upstream_transport)(request.clone());
        if (200..300).contains(&response.status) {
            self.observe_storefront_collection_value(&response.body["data"]);
        }
    }

    fn observe_storefront_collection_value(&mut self, value: &Value) {
        if value
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| is_shopify_gid_of_type(id, "Collection"))
        {
            let mut observed = value.clone();
            observed["__storefrontVisible"] = json!(true);
            let observed_products = storefront_collection_observed_products(&observed);
            if !observed_products.is_empty() {
                observed["products"] = connection_json(observed_products);
                if value.get("products").is_some() {
                    observed[STOREFRONT_CAPTURED_COLLECTION_DEFAULT_ORDER_FIELD] = json!(true);
                }
            }
            let owner_id = observed
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            self.stage_collection_from_observed_json(&observed);
            let mut metafields = observed
                .get("metafields")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter(|value| value.is_object())
                .cloned()
                .collect::<Vec<_>>();
            if let Some(metafield) = observed.get("metafield").filter(|value| value.is_object()) {
                metafields.push(metafield.clone());
            }
            for metafield in &mut metafields {
                metafield["__storefrontPublic"] = json!(true);
            }
            if !metafields.is_empty() {
                self.stage_observed_owner_metafields(
                    &owner_id,
                    &json!({ "metafields": { "nodes": metafields } }),
                );
            }
        }
        match value {
            Value::Array(values) => {
                for value in values {
                    self.observe_storefront_collection_value(value);
                }
            }
            Value::Object(object) => {
                for value in object.values() {
                    self.observe_storefront_collection_value(value);
                }
            }
            _ => {}
        }
    }

    fn storefront_collection_field_json(
        &self,
        field: &RootFieldSelection,
        context: &StorefrontRequestContext,
    ) -> Value {
        let collection = resolved_string_field(&field.arguments, "id")
            .and_then(|id| self.store.collection_by_id(&id))
            .or_else(|| {
                resolved_string_field(&field.arguments, "handle")
                    .and_then(|handle| self.store.collection_by_handle(&handle))
            });
        self.storefront_visible_collection_json(collection, context, &field.selection)
    }

    fn storefront_collection_by_handle_field_json(
        &self,
        field: &RootFieldSelection,
        context: &StorefrontRequestContext,
    ) -> Value {
        let collection = resolved_string_field(&field.arguments, "handle")
            .and_then(|handle| self.store.collection_by_handle(&handle));
        self.storefront_visible_collection_json(collection, context, &field.selection)
    }

    fn storefront_visible_collection_json(
        &self,
        collection: Option<&Value>,
        context: &StorefrontRequestContext,
        selections: &[SelectedField],
    ) -> Value {
        let Some(collection) =
            collection.filter(|collection| self.storefront_collection_is_visible(collection))
        else {
            return Value::Null;
        };
        self.storefront_collection_json(collection, context, selections)
    }

    fn storefront_collection_is_visible(&self, collection: &Value) -> bool {
        let Some(id) = collection.get("id").and_then(Value::as_str) else {
            return false;
        };
        if let Some(publications) = self.store.staged.resource_publications.get(id) {
            if self.store.staged.current_channel_publication_resolved {
                return self.store.resource_is_published_on_current_publication(id);
            }
            return publications
                .iter()
                .any(|publication_id| self.store.has_publication_id(publication_id));
        }
        collection
            .get("__storefrontVisible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn storefront_collections_connection_json(
        &self,
        field: &RootFieldSelection,
        context: &StorefrontRequestContext,
    ) -> Value {
        selected_staged_connection_with_args(
            self.store
                .staged
                .collections
                .values()
                .filter(|collection| self.storefront_collection_is_visible(collection))
                .cloned()
                .collect(),
            &field.arguments,
            &field.selection,
            |collection, query| self.collection_search_decision(collection, query),
            |collection, sort_key| self.storefront_collection_sort_key(collection, sort_key),
            |collection, selections| {
                self.storefront_collection_json(collection, context, selections)
            },
            value_id_cursor,
        )
    }

    fn storefront_collection_sort_key(
        &self,
        collection: &Value,
        sort_key: Option<&str>,
    ) -> StagedSortKey {
        if sort_key != Some("UPDATED_AT") {
            return collection_staged_sort_key(collection, sort_key);
        }

        let has_hidden_member = collection
            .pointer("/products/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|product| product.get("id").and_then(Value::as_str))
            .any(|id| {
                self.store.product_is_tombstoned(id)
                    || self
                        .store
                        .product_by_id(id)
                        .is_some_and(|product| !self.storefront_product_is_visible(product))
            });
        let projected_updated_at = if has_hidden_member {
            collection.get("updatedAt")
        } else {
            collection
                .get(STOREFRONT_COLLECTION_BASELINE_UPDATED_AT_FIELD)
                .or_else(|| collection.get("updatedAt"))
        }
        .and_then(Value::as_str)
        .unwrap_or_default();
        let id = collection
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        vec![
            StagedSortValue::String(projected_updated_at.to_string()),
            resource_id_tail_sort_value(Some(id)),
        ]
    }

    fn storefront_collection_json(
        &self,
        collection: &Value,
        context: &StorefrontRequestContext,
        selections: &[SelectedField],
    ) -> Value {
        let owner_id = collection
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        selected_payload_json(selections, |selection| {
            if !selection_applies_to_type(selection, "Collection") {
                return None;
            }
            match selection.name.as_str() {
                "__typename" => Some(json!("Collection")),
                "description" => Some(json!(storefront_collection_description(
                    collection, selection,
                ))),
                "descriptionHtml" => Some(json!(collection
                    .get("descriptionHtml")
                    .and_then(Value::as_str)
                    .unwrap_or_default())),
                "image" => Some(
                    collection
                        .get("image")
                        .map(|image| nullable_selected_json(image, &selection.selection))
                        .unwrap_or(Value::Null),
                ),
                "seo" => Some(selected_json(
                    &storefront_collection_seo(collection),
                    &selection.selection,
                )),
                "metafield" => Some(self.storefront_owner_metafield_json(owner_id, selection)),
                "metafields" => Some(self.storefront_owner_metafields_json(owner_id, selection)),
                "products" => Some(self.storefront_collection_products_connection_json(
                    collection, context, selection,
                )),
                _ => collection
                    .get(&selection.name)
                    .map(|value| nullable_selected_json(value, &selection.selection)),
            }
        })
    }

    fn storefront_collection_products_connection_json(
        &self,
        collection: &Value,
        context: &StorefrontRequestContext,
        selection: &SelectedField,
    ) -> Value {
        let filters = resolved_object_list_field(&selection.arguments, "filters");
        let mut products = self
            .collection_product_entries(collection)
            .into_iter()
            .filter(|entry| self.storefront_product_is_visible(&entry.product))
            .filter(|entry| storefront_collection_product_matches_filters(entry, &filters))
            .collect::<Vec<_>>();
        let requested_sort_key = resolved_string_field(&selection.arguments, "sortKey");
        let sort_key = if matches!(
            requested_sort_key.as_deref(),
            None | Some("COLLECTION_DEFAULT")
        ) && collection
            .get(STOREFRONT_CAPTURED_COLLECTION_DEFAULT_ORDER_FIELD)
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            Some("MANUAL")
        } else {
            requested_sort_key.as_deref()
        };
        let reverse = resolved_bool_field(&selection.arguments, "reverse").unwrap_or(false);
        sort_collection_product_entries(collection, &mut products, sort_key, reverse);
        selected_typed_connection_with_args(
            &products,
            &selection.arguments,
            &selection.selection,
            |entry, selections| {
                storefront_product_json(self, &entry.product, &entry.variants, context, selections)
            },
            collection_product_cursor,
        )
    }

    fn storefront_product_field_json(
        &self,
        field: &RootFieldSelection,
        context: &StorefrontRequestContext,
    ) -> Value {
        let product = resolved_string_field(&field.arguments, "id")
            .and_then(|id| self.store.product_by_id(&id))
            .or_else(|| {
                resolved_string_field(&field.arguments, "handle")
                    .and_then(|handle| self.store.product_by_handle(&handle))
            });
        self.storefront_visible_product_json(product, context, &field.selection)
    }

    fn storefront_product_by_handle_field_json(
        &self,
        field: &RootFieldSelection,
        context: &StorefrontRequestContext,
    ) -> Value {
        let product = resolved_string_field(&field.arguments, "handle")
            .and_then(|handle| self.store.product_by_handle(&handle));
        self.storefront_visible_product_json(product, context, &field.selection)
    }

    fn storefront_visible_product_json(
        &self,
        product: Option<&ProductRecord>,
        context: &StorefrontRequestContext,
        selections: &[SelectedField],
    ) -> Value {
        let Some(product) = product.filter(|product| self.storefront_product_is_visible(product))
        else {
            return Value::Null;
        };
        let variants = self.store.product_variants_for_product(&product.id);
        storefront_product_json(self, product, &variants, context, selections)
    }

    fn storefront_products_connection_json(
        &self,
        field: &RootFieldSelection,
        context: &StorefrontRequestContext,
    ) -> Value {
        selected_staged_connection_with_args(
            self.storefront_visible_products(),
            &field.arguments,
            &field.selection,
            |product, query| self.storefront_product_search_decision(product, query),
            |product, sort_key| self.storefront_product_sort_key(product, sort_key),
            |product, selections| {
                let variants = self.store.product_variants_for_product(&product.id);
                storefront_product_json(self, product, &variants, context, selections)
            },
            |product| product_cursor(product).to_string(),
        )
    }

    fn storefront_product_recommendations_json(
        &self,
        field: &RootFieldSelection,
        context: &StorefrontRequestContext,
    ) -> Value {
        let source = resolved_string_field(&field.arguments, "productId")
            .and_then(|id| self.store.product_by_id(&id))
            .or_else(|| {
                resolved_string_field(&field.arguments, "productHandle")
                    .and_then(|handle| self.store.product_by_handle(&handle))
            })
            .filter(|product| self.storefront_product_is_visible(product));
        let Some(source) = source else {
            return Value::Null;
        };
        let mut candidates = self
            .storefront_visible_products()
            .into_iter()
            .filter(|candidate| candidate.id != source.id)
            .map(|candidate| {
                let shared_tags = candidate
                    .tags
                    .iter()
                    .filter(|tag| source.tags.iter().any(|source_tag| source_tag == *tag))
                    .count();
                let score = shared_tags * 4
                    + usize::from(
                        !source.product_type.is_empty()
                            && candidate.product_type == source.product_type,
                    ) * 3
                    + usize::from(!source.vendor.is_empty() && candidate.vendor == source.vendor)
                        * 2;
                (score, candidate)
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|(left_score, left), (right_score, right)| {
            right_score
                .cmp(left_score)
                .then_with(|| {
                    left.title
                        .to_ascii_lowercase()
                        .cmp(&right.title.to_ascii_lowercase())
                })
                .then_with(|| left.id.cmp(&right.id))
        });
        Value::Array(
            candidates
                .into_iter()
                .take(10)
                .map(|(_, product)| {
                    let variants = self.store.product_variants_for_product(&product.id);
                    storefront_product_json(self, &product, &variants, context, &field.selection)
                })
                .collect(),
        )
    }

    fn storefront_product_taxonomy_connection_json(
        &self,
        field: &RootFieldSelection,
        kind: StorefrontProductTaxonomyKind,
    ) -> Value {
        let observed = match kind {
            StorefrontProductTaxonomyKind::Tag => &self.store.base.storefront_product_tags,
            StorefrontProductTaxonomyKind::ProductType => &self.store.base.storefront_product_types,
        };
        let mut values = if observed.is_object() {
            connection_nodes(observed)
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        } else {
            self.storefront_visible_products()
                .into_iter()
                .flat_map(|product| match kind {
                    StorefrontProductTaxonomyKind::Tag => product.tags,
                    StorefrontProductTaxonomyKind::ProductType => vec![product.product_type],
                })
                .collect::<Vec<_>>()
        };
        values.sort_by(|left, right| {
            left.to_ascii_lowercase()
                .cmp(&right.to_ascii_lowercase())
                .then_with(|| left.cmp(right))
        });
        values.dedup();
        selected_connection_json_with_args(
            values.into_iter().map(Value::String).collect(),
            &field.arguments,
            &field.selection,
            |value| {
                base64::engine::general_purpose::STANDARD
                    .encode(value.as_str().unwrap_or_default().as_bytes())
            },
        )
    }

    fn storefront_context_localization(
        &self,
        context: &StorefrontRequestContext,
    ) -> Option<&Value> {
        self.store
            .base
            .storefront_localizations
            .get(&context.key())
            .or_else(|| {
                context.country.as_deref().and_then(|country_code| {
                    self.store
                        .base
                        .storefront_localizations
                        .values()
                        .find(|localization| {
                            localization
                                .pointer("/country/isoCode")
                                .and_then(Value::as_str)
                                == Some(country_code)
                        })
                })
            })
            .or_else(|| {
                self.store
                    .base
                    .storefront_localizations
                    .get(STOREFRONT_DEFAULT_CONTEXT_KEY)
            })
    }

    fn storefront_context_price_list(&self, context: &StorefrontRequestContext) -> Option<&Value> {
        let localization = self.storefront_context_localization(context)?;
        let observed_market_id = localization.pointer("/market/id").and_then(Value::as_str);
        let observed_market_handle = localization
            .pointer("/market/handle")
            .and_then(Value::as_str);
        let market_id = self
            .store
            .staged
            .markets
            .iter()
            .find_map(|(id, market)| {
                (market.get("handle").and_then(Value::as_str) == observed_market_handle)
                    .then_some(id.as_str())
            })
            .or(observed_market_id)?;
        let catalog = self.store.staged.catalogs.values().find(|catalog| {
            catalog.get("status").and_then(Value::as_str) == Some("ACTIVE")
                && catalog_market_ids(catalog).iter().any(|id| id == market_id)
        })?;
        let price_list_id = catalog_relation_id(catalog, "priceListId", "priceList")?;
        self.store.staged.price_lists.get(&price_list_id)
    }

    pub(in crate::proxy) fn storefront_variant_pricing(
        &self,
        variant: &ProductVariantRecord,
        context: &StorefrontRequestContext,
    ) -> StorefrontVariantPricing {
        let contextual_price_list = self.storefront_context_price_list(context);
        let fixed_price = contextual_price_list.and_then(|price_list| {
            price_edges(price_list).into_iter().find_map(|edge| {
                (fixed_price_edge_variant_id(&edge).as_deref() == Some(variant.id.as_str()))
                    .then(|| edge.get("node").cloned())
                    .flatten()
            })
        });
        let currency_code = self
            .storefront_context_localization(context)
            .and_then(|localization| localization.pointer("/country/currency/isoCode"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| contextual_price_list.map(price_list_currency))
            .or_else(|| self.store.observed_shop_currency_code())
            .or_else(|| {
                variant
                    .extra_fields
                    .get("currencyCode")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_default();
        StorefrontVariantPricing {
            price: fixed_price
                .as_ref()
                .and_then(|price| price.pointer("/price/amount"))
                .and_then(Value::as_str)
                .unwrap_or(&variant.price)
                .to_string(),
            compare_at_price: fixed_price
                .as_ref()
                .and_then(|price| price.pointer("/compareAtPrice/amount"))
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| variant.compare_at_price.clone()),
            currency_code,
        }
    }

    fn storefront_visible_products(&self) -> Vec<ProductRecord> {
        self.store
            .products()
            .into_iter()
            .filter(|product| self.storefront_product_is_visible(product))
            .collect()
    }

    pub(in crate::proxy) fn storefront_product_is_visible(&self, product: &ProductRecord) -> bool {
        if product.status != "ACTIVE" {
            return false;
        }
        if let Some(publications) = self.store.staged.resource_publications.get(&product.id) {
            if self.store.staged.current_channel_publication_resolved {
                return self
                    .store
                    .product_is_published_on_current_publication(product);
            }
            return publications
                .iter()
                .any(|publication_id| self.store.has_publication_id(publication_id));
        }
        if product
            .extra_fields
            .get("__storefrontVisible")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            return true;
        }
        if self.store.staged.current_channel_publication_resolved {
            return self
                .store
                .product_is_published_on_current_publication(product);
        }
        self.store
            .product_is_published_on_known_publication(product)
    }

    fn storefront_product_search_decision(
        &self,
        product: &ProductRecord,
        query: Option<&str>,
    ) -> StagedSearchDecision {
        let Some(query) = query else {
            return StagedSearchDecision::Match;
        };
        let variants = self.store.product_variants_for_product(&product.id);
        StagedSearchDecision::from_bool(product_matches_search_query(product, &variants, query))
    }

    fn storefront_product_sort_key(
        &self,
        product: &ProductRecord,
        sort_key: Option<&str>,
    ) -> StagedSortKey {
        let variants = self.store.product_variants_for_product(&product.id);
        storefront_product_sort_key(product, &variants, sort_key)
    }

    pub(in crate::proxy) fn storefront_currency_code(&self) -> String {
        self.store
            .observed_shop_currency_code()
            .unwrap_or_else(|| "USD".to_string())
    }
    fn storefront_shop_json(&self, selections: &[SelectedField]) -> Value {
        let storefront_shop = self.store.base.storefront_shop.clone();
        let admin_shop = if self.store.base.shop.is_object() {
            self.store.effective_shop()
        } else {
            Value::Null
        };
        let has_shop = storefront_shop.is_object()
            || admin_shop.is_object()
            || self.storefront_shop_selection_uses_only_local_metafields(selections);
        if !has_shop {
            return Value::Null;
        }
        selected_payload_json(selections, |selection| {
            if !selection_applies_to_type(selection, "Shop") {
                return None;
            }
            match selection.name.as_str() {
                "__typename" => Some(json!("Shop")),
                "paymentSettings" => {
                    Some(self.storefront_payment_settings_json(&selection.selection))
                }
                "primaryDomain" => self
                    .storefront_shop_field(&storefront_shop, &admin_shop, "primaryDomain")
                    .map(|domain| selected_json(&domain, &selection.selection)),
                "privacyPolicy" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "privacyPolicy",
                    "PRIVACY_POLICY",
                    &selection.selection,
                ),
                "refundPolicy" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "refundPolicy",
                    "REFUND_POLICY",
                    &selection.selection,
                ),
                "shippingPolicy" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "shippingPolicy",
                    "SHIPPING_POLICY",
                    &selection.selection,
                ),
                "termsOfService" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "termsOfService",
                    "TERMS_OF_SERVICE",
                    &selection.selection,
                ),
                "termsOfSale" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "termsOfSale",
                    "TERMS_OF_SALE",
                    &selection.selection,
                ),
                "legalNotice" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "legalNotice",
                    "LEGAL_NOTICE",
                    &selection.selection,
                ),
                "contactInformation" => self.storefront_shop_policy_json(
                    &storefront_shop,
                    "contactInformation",
                    "CONTACT_INFORMATION",
                    &selection.selection,
                ),
                "moneyFormat" => self
                    .storefront_shop_field(&storefront_shop, &admin_shop, "moneyFormat")
                    .or_else(|| self.store.shop_money_format().map(Value::String)),
                "metafield" => Some(self.storefront_shop_metafield_json(selection)),
                "metafields" => Some(self.storefront_shop_metafields_json(selection)),
                _ => self
                    .storefront_shop_field(&storefront_shop, &admin_shop, &selection.name)
                    .map(|value| nullable_selected_json(&value, &selection.selection))
                    .or(Some(Value::Null)),
            }
        })
    }

    fn storefront_shop_field(
        &self,
        storefront_shop: &Value,
        admin_shop: &Value,
        field: &str,
    ) -> Option<Value> {
        storefront_shop
            .get(field)
            .cloned()
            .or_else(|| admin_shop.get(field).cloned())
    }

    fn storefront_shop_policy_json(
        &self,
        storefront_shop: &Value,
        storefront_field: &str,
        policy_type: &str,
        selections: &[SelectedField],
    ) -> Option<Value> {
        if let Some(policy) = storefront_shop.get(storefront_field) {
            return Some(nullable_selected_json(policy, selections));
        }
        let policy = self.store.shop_policy_by_type(policy_type)?;
        Some(selected_json(
            &storefront_policy_from_admin(policy),
            selections,
        ))
    }

    fn storefront_localization_is_observed(&self, context: &StorefrontRequestContext) -> bool {
        self.store
            .base
            .storefront_localizations
            .contains_key(&context.key())
    }

    fn storefront_localization_json(
        &self,
        context: &StorefrontRequestContext,
        selections: &[SelectedField],
    ) -> Value {
        self.store
            .base
            .storefront_localizations
            .get(&context.key())
            .map(|localization| selected_json(localization, selections))
            .unwrap_or(Value::Null)
    }

    fn storefront_payment_settings_json(&self, selections: &[SelectedField]) -> Value {
        self.storefront_payment_settings_source()
            .map(|settings| selected_json(&settings, selections))
            .unwrap_or(Value::Null)
    }

    fn storefront_payment_settings_source(&self) -> Option<Value> {
        if self.store.base.storefront_payment_settings.is_object() {
            return Some(self.store.base.storefront_payment_settings.clone());
        }
        self.admin_storefront_payment_settings_source()
    }

    fn admin_storefront_payment_settings_source(&self) -> Option<Value> {
        let shop = self.store.effective_shop();
        let mut settings = serde_json::Map::new();
        if let Some(value) = shop.pointer("/paymentSettings/supportedDigitalWallets") {
            settings.insert("supportedDigitalWallets".to_string(), value.clone());
        }
        if let Some(value) = shop.get("currencyCode") {
            settings.insert("currencyCode".to_string(), value.clone());
        }
        if let Some(value) = shop.get("enabledPresentmentCurrencies") {
            settings.insert("enabledPresentmentCurrencies".to_string(), value.clone());
        }
        if let Some(value) = shop.pointer("/shopAddress/countryCodeV2") {
            settings.insert("countryCode".to_string(), value.clone());
        }
        (!settings.is_empty()).then_some(Value::Object(settings))
    }

    fn storefront_locations_connection_json(
        &self,
        arguments: &BTreeMap<String, ResolvedValue>,
        selections: &[SelectedField],
    ) -> Value {
        let mut records = self.storefront_location_records();
        sort_storefront_locations(&mut records, arguments);
        let cursor_by_id = self.storefront_location_cursor_map(&records);
        let (records, page_info) = connection_window(&records, arguments, |location| {
            storefront_location_cursor(location, &cursor_by_id)
        });
        selected_typed_connection_with_page_info(
            &records,
            selections,
            selected_json,
            |location| storefront_location_cursor(location, &cursor_by_id),
            page_info,
        )
    }

    fn storefront_location_records(&self) -> Vec<Value> {
        let mut records = Vec::new();
        let mut seen = BTreeSet::new();
        for location in self.store.base.storefront_locations.ordered_values() {
            push_storefront_location(&mut records, &mut seen, location.clone());
        }
        for id in &self.store.staged.observed_shipping_location_order {
            if let Some(location) = self.store.staged.observed_shipping_locations.get(id) {
                push_admin_location_as_storefront(&mut records, &mut seen, location);
            }
        }
        for location in self.store.staged.observed_shipping_locations.values() {
            push_admin_location_as_storefront(&mut records, &mut seen, location);
        }
        for id in &self.store.staged.locations.order {
            if let Some(location) = self.store.staged.locations.get(id) {
                push_admin_location_as_storefront(&mut records, &mut seen, location);
            }
        }
        for (_, location) in self.store.staged.locations.iter() {
            push_admin_location_as_storefront(&mut records, &mut seen, location);
        }
        records
    }

    fn storefront_location_cursor_map(&self, records: &[Value]) -> BTreeMap<String, String> {
        records
            .iter()
            .filter_map(|location| {
                let id = location.get("id").and_then(Value::as_str)?;
                let cursor = self
                    .store
                    .base
                    .storefront_location_cursors
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| id.to_string());
                Some((id.to_string(), cursor))
            })
            .collect()
    }

    fn storefront_article_root(&self, field: &RootFieldSelection) -> Value {
        let id = resolved_string_field(&field.arguments, "id").unwrap_or_default();
        self.storefront_content_by_id(StorefrontContentKind::Article, &id)
            .map(|article| self.selected_storefront_article(&article, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_blog_root(&self, field: &RootFieldSelection) -> Value {
        let record = resolved_string_field(&field.arguments, "id")
            .and_then(|id| self.storefront_content_by_id(StorefrontContentKind::Blog, &id))
            .or_else(|| {
                resolved_string_field(&field.arguments, "handle").and_then(|handle| {
                    self.storefront_content_by_handle(StorefrontContentKind::Blog, &handle)
                })
            });
        record
            .map(|blog| self.selected_storefront_blog(&blog, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_blog_by_handle_root(&self, field: &RootFieldSelection) -> Value {
        let handle = resolved_string_field(&field.arguments, "handle").unwrap_or_default();
        self.storefront_content_by_handle(StorefrontContentKind::Blog, &handle)
            .map(|blog| self.selected_storefront_blog(&blog, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_page_root(&self, field: &RootFieldSelection) -> Value {
        let record = resolved_string_field(&field.arguments, "id")
            .and_then(|id| self.storefront_content_by_id(StorefrontContentKind::Page, &id))
            .or_else(|| {
                resolved_string_field(&field.arguments, "handle").and_then(|handle| {
                    self.storefront_content_by_handle(StorefrontContentKind::Page, &handle)
                })
            });
        record
            .map(|page| self.selected_storefront_page(&page, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_page_by_handle_root(&self, field: &RootFieldSelection) -> Value {
        let handle = resolved_string_field(&field.arguments, "handle").unwrap_or_default();
        self.storefront_content_by_handle(StorefrontContentKind::Page, &handle)
            .map(|page| self.selected_storefront_page(&page, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_content_by_id(&self, kind: StorefrontContentKind, id: &str) -> Option<Value> {
        self.storefront_content_records(kind)
            .into_iter()
            .find(|record| record.get("id").and_then(Value::as_str) == Some(id))
    }

    fn storefront_content_by_handle(
        &self,
        kind: StorefrontContentKind,
        handle: &str,
    ) -> Option<Value> {
        self.storefront_content_records(kind)
            .into_iter()
            .find(|record| record.get("handle").and_then(Value::as_str) == Some(handle))
    }

    fn storefront_content_records(&self, kind: StorefrontContentKind) -> Vec<Value> {
        match kind {
            StorefrontContentKind::Blog => self.storefront_blog_records(),
            StorefrontContentKind::Page => self.storefront_page_records(),
            StorefrontContentKind::Article => self.storefront_article_records(),
        }
    }

    fn storefront_blog_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_blog_order
            .iter()
            .filter(|id| {
                !self
                    .store
                    .staged
                    .deleted_online_store_blog_ids
                    .contains(*id)
            })
            .filter_map(|id| self.store.staged.online_store_blogs.get(id))
            .map(storefront_blog_record_from_admin)
            .collect()
    }

    fn storefront_page_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_page_order
            .iter()
            .filter(|id| {
                !self
                    .store
                    .staged
                    .deleted_online_store_page_ids
                    .contains(*id)
            })
            .filter_map(|id| self.store.staged.online_store_pages.get(id))
            .filter(|page| storefront_content_is_visible(page))
            .map(storefront_page_record_from_admin)
            .collect()
    }

    fn storefront_article_records(&self) -> Vec<Value> {
        self.store
            .staged
            .online_store_article_order
            .iter()
            .filter(|id| {
                !self
                    .store
                    .staged
                    .deleted_online_store_article_ids
                    .contains(*id)
            })
            .filter_map(|id| self.store.staged.online_store_articles.get(id))
            .filter(|article| storefront_content_is_visible(article))
            .filter(|article| {
                article
                    .get("blogId")
                    .and_then(Value::as_str)
                    .and_then(|blog_id| {
                        self.storefront_content_by_id(StorefrontContentKind::Blog, blog_id)
                    })
                    .is_some()
            })
            .map(storefront_article_record_from_admin)
            .collect()
    }

    fn storefront_articles_for_blog(&self, blog_id: &str) -> Vec<Value> {
        self.storefront_article_records()
            .into_iter()
            .filter(|article| article.get("blogId").and_then(Value::as_str) == Some(blog_id))
            .collect()
    }

    fn selected_storefront_blog(&self, blog: &Value, selection: &[SelectedField]) -> Value {
        let blog_id = blog.get("id").and_then(Value::as_str).unwrap_or_default();
        selected_payload_json(selection, |field| {
            if !selection_applies_to_type(field, "Blog") {
                return None;
            }
            match field.name.as_str() {
                "articleByHandle" => {
                    let handle =
                        resolved_string_field(&field.arguments, "handle").unwrap_or_default();
                    self.storefront_articles_for_blog(blog_id)
                        .into_iter()
                        .find(|article| {
                            article.get("handle").and_then(Value::as_str) == Some(&handle)
                        })
                        .map(|article| self.selected_storefront_article(&article, &field.selection))
                        .or(Some(Value::Null))
                }
                "articles" => Some(self.storefront_content_connection(
                    StorefrontContentKind::Article,
                    self.storefront_articles_for_blog(blog_id),
                    &field.arguments,
                    &field.selection,
                )),
                "authors" => {
                    let mut seen = BTreeSet::new();
                    let authors = self
                        .storefront_articles_for_blog(blog_id)
                        .into_iter()
                        .filter_map(|article| article.get("author").cloned())
                        .filter(|author| {
                            let name = author
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or_default();
                            !name.is_empty() && seen.insert(name.to_string())
                        })
                        .map(|author| selected_json(&author, &field.selection))
                        .collect::<Vec<_>>();
                    Some(Value::Array(authors))
                }
                "metafield" => Some(Value::Null),
                "metafields" => Some(storefront_metafields_list(
                    &field.arguments,
                    &field.selection,
                )),
                "onlineStoreUrl" => Some(Value::Null),
                "seo" => Some(selected_json(&storefront_default_seo(), &field.selection)),
                _ => selected_field_json(blog, field).or(Some(Value::Null)),
            }
        })
    }

    fn selected_storefront_article(&self, article: &Value, selection: &[SelectedField]) -> Value {
        selected_payload_json(selection, |field| {
            if !selection_applies_to_type(field, "Article") {
                return None;
            }
            match field.name.as_str() {
                "author" | "authorV2" => article
                    .get("author")
                    .map(|author| selected_json(author, &field.selection))
                    .or(Some(Value::Null)),
                "blog" => article
                    .get("blogId")
                    .and_then(Value::as_str)
                    .and_then(|blog_id| {
                        self.storefront_content_by_id(StorefrontContentKind::Blog, blog_id)
                    })
                    .map(|blog| self.selected_storefront_blog(&blog, &field.selection)),
                "comments" => Some(selected_empty_connection_json(&field.selection)),
                "content" => Some(json!(storefront_truncated_text(
                    article
                        .get("content")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                    &field.arguments,
                ))),
                "contentHtml" => selected_field_json(article, field).or(Some(json!(""))),
                "excerpt" => Some(
                    article
                        .get("excerpt")
                        .and_then(Value::as_str)
                        .map(|excerpt| json!(storefront_truncated_text(excerpt, &field.arguments)))
                        .unwrap_or(Value::Null),
                ),
                "excerptHtml" => selected_field_json(article, field).or(Some(Value::Null)),
                "metafield" => Some(Value::Null),
                "metafields" => Some(storefront_metafields_list(
                    &field.arguments,
                    &field.selection,
                )),
                "onlineStoreUrl" | "trackingParameters" => Some(Value::Null),
                "seo" => Some(selected_json(&storefront_default_seo(), &field.selection)),
                _ => selected_field_json(article, field).or(Some(Value::Null)),
            }
        })
    }

    fn selected_storefront_page(&self, page: &Value, selection: &[SelectedField]) -> Value {
        selected_payload_json(selection, |field| {
            if !selection_applies_to_type(field, "Page") {
                return None;
            }
            match field.name.as_str() {
                "metafield" => Some(Value::Null),
                "metafields" => Some(storefront_metafields_list(
                    &field.arguments,
                    &field.selection,
                )),
                "onlineStoreUrl" | "trackingParameters" => Some(Value::Null),
                "seo" => Some(selected_json(&storefront_default_seo(), &field.selection)),
                _ => selected_field_json(page, field).or(Some(Value::Null)),
            }
        })
    }

    fn storefront_content_connection(
        &self,
        kind: StorefrontContentKind,
        records: Vec<Value>,
        arguments: &BTreeMap<String, ResolvedValue>,
        selection: &[SelectedField],
    ) -> Value {
        let result = staged_connection_query(
            records,
            arguments,
            |record, query| storefront_content_search_decision(kind, record, query),
            |record, sort_key| storefront_content_sort_key(kind, record, sort_key),
            value_id_cursor,
        );
        selected_typed_connection_with_page_info(
            &result.records,
            selection,
            |record, selection| match kind {
                StorefrontContentKind::Blog => self.selected_storefront_blog(record, selection),
                StorefrontContentKind::Page => self.selected_storefront_page(record, selection),
                StorefrontContentKind::Article => {
                    self.selected_storefront_article(record, selection)
                }
            },
            value_id_cursor,
            result.page_info,
        )
    }

    fn storefront_menu_root(&self, field: &RootFieldSelection) -> Value {
        let handle = resolved_string_field(&field.arguments, "handle").unwrap_or_default();
        self.storefront_menu_by_handle(&handle)
            .map(|menu| selected_json(&menu, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_menu_by_handle(&self, handle: &str) -> Option<Value> {
        self.store
            .base
            .storefront_menus
            .ordered_values()
            .into_iter()
            .find(|menu| menu.get("handle").and_then(Value::as_str) == Some(handle))
            .cloned()
    }

    fn storefront_sitemap_root(&self, field: &RootFieldSelection) -> Value {
        let sitemap_type = resolved_string_field(&field.arguments, "type").unwrap_or_default();
        let resources = self.storefront_sitemap_resources(&sitemap_type);
        selected_payload_json(&field.selection, |selection| {
            match selection.name.as_str() {
                "pagesCount" => Some(selected_json(
                    &count_object(resources.len()),
                    &selection.selection,
                )),
                "resources" => Some(storefront_selected_sitemap_resources(
                    &resources,
                    &selection.arguments,
                    &selection.selection,
                )),
                _ => None,
            }
        })
    }

    fn storefront_sitemap_resources(&self, sitemap_type: &str) -> Vec<Value> {
        let records = match sitemap_type {
            "ARTICLE" => self.storefront_article_records(),
            "BLOG" => self.storefront_blog_records(),
            "PAGE" => self.storefront_page_records(),
            _ => Vec::new(),
        };
        records
            .into_iter()
            .map(|record| {
                json!({
                    "__typename": "SitemapResource",
                    "handle": record.get("handle").cloned().unwrap_or(Value::Null),
                    "title": record.get("title").cloned().unwrap_or(Value::Null),
                    "updatedAt": record
                        .get("updatedAt")
                        .or_else(|| record.get("publishedAt"))
                        .cloned()
                        .unwrap_or(Value::Null),
                    "image": record.get("image").cloned().unwrap_or(Value::Null)
                })
            })
            .collect()
    }

    fn storefront_metaobject_root_json(&self, field: &RootFieldSelection) -> Value {
        let record = if let Some(id) = resolved_string_field(&field.arguments, "id") {
            self.metaobject_by_id(&id)
        } else if let Some(handle) = resolved_object_field(&field.arguments, "handle") {
            let meta_type = resolved_string_field(&handle, "type").unwrap_or_default();
            let meta_handle = resolved_string_field(&handle, "handle").unwrap_or_default();
            self.metaobject_by_type_and_handle(&meta_type, &meta_handle)
        } else {
            None
        };
        record
            .and_then(|record| self.storefront_visible_metaobject(&record))
            .map(|record| self.storefront_selected_metaobject(&record, &field.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_metaobjects_connection_json(&self, field: &RootFieldSelection) -> Value {
        let meta_type = resolved_string_field(&field.arguments, "type").unwrap_or_default();
        let records =
            self.store
                .staged
                .metaobjects
                .values()
                .filter(|record| {
                    record.get("type").and_then(Value::as_str) == Some(meta_type.as_str())
                        && !self.store.staged.metaobjects.is_tombstoned(
                            record.get("id").and_then(Value::as_str).unwrap_or_default(),
                        )
                })
                .filter_map(|record| self.storefront_visible_metaobject(record))
                .filter(|record| self.metaobject_visible_in_catalog(record))
                .collect::<Vec<_>>();
        selected_staged_connection_with_args(
            records,
            &field.arguments,
            &field.selection,
            |_record, _query| StagedSearchDecision::Match,
            storefront_metaobject_sort_key,
            |record, selections| self.storefront_selected_metaobject(record, selections),
            metaobject_cursor,
        )
    }

    fn storefront_visible_metaobject(&self, record: &Value) -> Option<Value> {
        let projected = self.project_metaobject_against_definition(record);
        let meta_type = projected.get("type").and_then(Value::as_str)?;
        let definition = self.metaobject_definition_by_type(meta_type)?;
        if definition
            .pointer("/access/storefront")
            .and_then(Value::as_str)
            != Some("PUBLIC_READ")
        {
            return None;
        }
        if definition
            .pointer("/capabilities/publishable/enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            && projected
                .pointer("/capabilities/publishable/status")
                .and_then(Value::as_str)
                != Some("ACTIVE")
        {
            return None;
        }
        Some(projected)
    }

    fn storefront_selected_metaobject(
        &self,
        record: &Value,
        selections: &[SelectedField],
    ) -> Value {
        selected_payload_json(selections, |selection| {
            if !selection_applies_to_type(selection, "Metaobject") {
                return None;
            }
            match selection.name.as_str() {
                "__typename" => Some(json!("Metaobject")),
                "field" => {
                    let key =
                        resolved_string_field(&selection.arguments, "key").unwrap_or_default();
                    let field =
                        record["fields"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|candidate| {
                                candidate.get("key").and_then(Value::as_str) == Some(key.as_str())
                            })?;
                    Some(self.storefront_selected_metaobject_field(field, &selection.selection))
                }
                "fields" => {
                    let fields = storefront_metaobject_fields(record);
                    Some(Value::Array(
                        fields
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|field| {
                                self.storefront_selected_metaobject_field(
                                    field,
                                    &selection.selection,
                                )
                            })
                            .collect(),
                    ))
                }
                "onlineStoreUrl" | "seo" => Some(
                    record
                        .get(&selection.name)
                        .map(|value| nullable_selected_json(value, &selection.selection))
                        .unwrap_or(Value::Null),
                ),
                _ => record
                    .get(&selection.name)
                    .map(|value| nullable_selected_json(value, &selection.selection)),
            }
        })
    }

    fn storefront_selected_metaobject_field(
        &self,
        record: &Value,
        selections: &[SelectedField],
    ) -> Value {
        selected_payload_json(selections, |selection| match selection.name.as_str() {
            "reference" => Some(self.storefront_selected_scalar_reference_json(record, selection)),
            "references" => {
                Some(self.storefront_selected_reference_connection_json(record, selection))
            }
            _ => record
                .get(&selection.name)
                .map(|value| nullable_selected_json(value, &selection.selection)),
        })
    }

    fn storefront_shop_metafield_json(&self, selection: &SelectedField) -> Value {
        let Some(owner_id) = self.storefront_shop_owner_id() else {
            return Value::Null;
        };
        self.storefront_owner_metafield_json(&owner_id, selection)
    }

    fn storefront_owner_metafield_json(&self, owner_id: &str, selection: &SelectedField) -> Value {
        let namespace =
            resolved_string_field(&selection.arguments, "namespace").unwrap_or_default();
        let key = resolved_string_field(&selection.arguments, "key").unwrap_or_default();
        self.storefront_owner_metafield(owner_id, &namespace, &key)
            .map(|metafield| self.storefront_selected_metafield(&metafield, &selection.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_shop_metafields_json(&self, selection: &SelectedField) -> Value {
        let Some(owner_id) = self.storefront_shop_owner_id() else {
            return Value::Array(Vec::new());
        };
        self.storefront_owner_metafields_json(&owner_id, selection)
    }

    fn storefront_owner_metafields_json(&self, owner_id: &str, selection: &SelectedField) -> Value {
        Value::Array(
            resolved_object_list_field(&selection.arguments, "identifiers")
                .into_iter()
                .map(|identifier| {
                    let namespace =
                        resolved_string_field(&identifier, "namespace").unwrap_or_default();
                    let key = resolved_string_field(&identifier, "key").unwrap_or_default();
                    self.storefront_owner_metafield(owner_id, &namespace, &key)
                        .map(|metafield| {
                            self.storefront_selected_metafield(&metafield, &selection.selection)
                        })
                        .unwrap_or(Value::Null)
                })
                .collect(),
        )
    }

    fn storefront_resource_metafield_json(
        &self,
        owner_id: &str,
        selection: &SelectedField,
    ) -> Value {
        let namespace =
            resolved_string_field(&selection.arguments, "namespace").unwrap_or_default();
        let key = resolved_string_field(&selection.arguments, "key").unwrap_or_default();
        self.storefront_owner_metafield(owner_id, &namespace, &key)
            .map(|metafield| self.storefront_selected_metafield(&metafield, &selection.selection))
            .unwrap_or(Value::Null)
    }

    fn storefront_resource_metafields_json(
        &self,
        owner_id: &str,
        selection: &SelectedField,
    ) -> Value {
        Value::Array(
            resolved_object_list_field(&selection.arguments, "identifiers")
                .into_iter()
                .map(|identifier| {
                    let namespace =
                        resolved_string_field(&identifier, "namespace").unwrap_or_default();
                    let key = resolved_string_field(&identifier, "key").unwrap_or_default();
                    self.storefront_owner_metafield(owner_id, &namespace, &key)
                        .map(|metafield| {
                            self.storefront_selected_metafield(&metafield, &selection.selection)
                        })
                        .unwrap_or(Value::Null)
                })
                .collect(),
        )
    }

    fn storefront_owner_metafield(
        &self,
        owner_id: &str,
        namespace: &str,
        key: &str,
    ) -> Option<Value> {
        let keys = vec![(namespace.to_string(), key.to_string())];
        self.owner_metafields(owner_id, Some(namespace), Some(&keys))
            .into_iter()
            .find(|metafield| {
                metafield.get("namespace").and_then(Value::as_str) == Some(namespace)
                    && metafield.get("key").and_then(Value::as_str) == Some(key)
            })
            .filter(storefront_metafield_is_public)
    }

    fn storefront_selected_metafield(&self, record: &Value, selections: &[SelectedField]) -> Value {
        selected_payload_json(selections, |selection| {
            if !selection_applies_to_type(selection, "Metafield") {
                return None;
            }
            match selection.name.as_str() {
                "__typename" => Some(json!("Metafield")),
                "list" => Some(json!(record
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|field_type| field_type.starts_with("list.")))),
                "description" => Some(Value::Null),
                "reference" => {
                    Some(self.storefront_selected_scalar_reference_json(record, selection))
                }
                "references" => {
                    Some(self.storefront_selected_reference_connection_json(record, selection))
                }
                "parentResource" => {
                    Some(self.storefront_selected_metafield_parent(record, selection))
                }
                _ => record
                    .get(&selection.name)
                    .map(|value| nullable_selected_json(value, &selection.selection)),
            }
        })
    }

    fn storefront_selected_metafield_parent(
        &self,
        record: &Value,
        selection: &SelectedField,
    ) -> Value {
        let Some(owner_id) = record.pointer("/owner/id").and_then(Value::as_str) else {
            return Value::Null;
        };
        self.storefront_selected_reference_node_json(owner_id, &selection.selection)
            .unwrap_or(Value::Null)
    }

    fn storefront_selected_scalar_reference_json(
        &self,
        record: &Value,
        selection: &SelectedField,
    ) -> Value {
        let Some(id) = scalar_reference_id(record) else {
            return Value::Null;
        };
        self.storefront_selected_reference_node_json(&id, &selection.selection)
            .unwrap_or(Value::Null)
    }

    fn storefront_selected_reference_connection_json(
        &self,
        record: &Value,
        selection: &SelectedField,
    ) -> Value {
        let ids = list_reference_ids(record)
            .into_iter()
            .filter(|id| {
                self.storefront_selected_reference_node_json(id, &[])
                    .is_some()
            })
            .collect::<Vec<_>>();
        let (ids, page_info) = connection_window(&ids, &selection.arguments, |id| id.clone());
        selected_typed_connection_with_page_info(
            &ids,
            &selection.selection,
            |id, selections| {
                self.storefront_selected_reference_node_json(id, selections)
                    .unwrap_or(Value::Null)
            },
            |id| id.clone(),
            page_info,
        )
    }

    fn storefront_selected_reference_node_json(
        &self,
        id: &str,
        selections: &[SelectedField],
    ) -> Option<Value> {
        match shopify_gid_resource_type(id) {
            Some("Metaobject") => {
                let record = self.metaobject_by_id(id)?;
                let record = self.storefront_visible_metaobject(&record)?;
                Some(self.storefront_selected_metaobject(&record, selections))
            }
            Some("Shop") => {
                let shop = self.store.effective_shop();
                (shop.get("id").and_then(Value::as_str) == Some(id))
                    .then(|| self.storefront_shop_json(selections))
            }
            _ => None,
        }
    }

    fn storefront_shop_owner_id(&self) -> Option<String> {
        self.store
            .effective_shop()
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                self.store
                    .staged
                    .owner_metafields
                    .keys()
                    .find(|id| shopify_gid_resource_type(id.as_str()) == Some("Shop"))
                    .cloned()
            })
    }
}

pub(in crate::proxy) fn storefront_discovery_argument_error(
    field: &RootFieldSelection,
) -> Option<(String, Value)> {
    if matches!(field.name.as_str(), "node" | "nodes") {
        let ids = if field.name == "node" {
            resolved_string_field(&field.arguments, "id")
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            list_string_field(&field.arguments, "ids")
        };
        if let Some(id) = ids
            .into_iter()
            .find(|id| shopify_gid_resource_type(id).is_none())
        {
            return Some((
                format!("Invalid global id '{id}'"),
                json!({ "code": "argumentLiteralsIncompatible", "typeName": "CoercionError" }),
            ));
        }
    }
    if field.name == "predictiveSearch"
        && resolved_int_field(&field.arguments, "limit")
            .is_some_and(|limit| !(1..=10).contains(&limit))
    {
        return Some((
            "limit must be within 1..10".to_string(),
            json!({ "code": "INVALID_FIELD_ARGUMENTS" }),
        ));
    }
    None
}

fn storefront_search_item_cursor(item: &StorefrontSearchItem) -> String {
    match item {
        StorefrontSearchItem::Product(product) => product.id.clone(),
        StorefrontSearchItem::Article(record) | StorefrontSearchItem::Page(record) => {
            value_id_cursor(record)
        }
    }
}

fn storefront_search_item_type_rank(item: &StorefrontSearchItem) -> u8 {
    match item {
        StorefrontSearchItem::Product(_) => 0,
        StorefrontSearchItem::Article(_) => 1,
        StorefrontSearchItem::Page(_) => 2,
    }
}

fn storefront_search_item_title(item: &StorefrontSearchItem) -> String {
    match item {
        StorefrontSearchItem::Product(product) => product.title.to_ascii_lowercase(),
        StorefrontSearchItem::Article(record) | StorefrontSearchItem::Page(record) => {
            storefront_value_title(record)
        }
    }
}

fn storefront_sort_search_items(
    proxy: &DraftProxy,
    items: &mut [StorefrontSearchItem],
    arguments: &BTreeMap<String, ResolvedValue>,
) {
    let sort_key =
        resolved_string_field(arguments, "sortKey").unwrap_or_else(|| "RELEVANCE".to_string());
    items.sort_by(|left, right| {
        let ordering = if sort_key == "PRICE" {
            storefront_search_item_price(left)
                .total_cmp(&storefront_search_item_price(right))
                .then_with(|| {
                    storefront_search_item_type_rank(left)
                        .cmp(&storefront_search_item_type_rank(right))
                })
        } else {
            storefront_search_item_type_rank(left)
                .cmp(&storefront_search_item_type_rank(right))
                .then_with(|| {
                    storefront_search_item_title(left).cmp(&storefront_search_item_title(right))
                })
        };
        ordering.then_with(|| {
            storefront_search_item_cursor(left).cmp(&storefront_search_item_cursor(right))
        })
    });
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        items.reverse();
    }
    if resolved_string_field(arguments, "unavailableProducts").as_deref() == Some("LAST") {
        items.sort_by_key(|item| match item {
            StorefrontSearchItem::Product(product) => {
                !storefront_search_product_available(proxy, product)
            }
            _ => false,
        });
    }
}

fn storefront_search_item_price(item: &StorefrontSearchItem) -> f64 {
    match item {
        StorefrontSearchItem::Product(product) => product
            .variants
            .iter()
            .filter_map(|variant| {
                variant
                    .get("price")
                    .and_then(Value::as_str)?
                    .parse::<f64>()
                    .ok()
            })
            .min_by(f64::total_cmp)
            .unwrap_or(0.0),
        _ => f64::INFINITY,
    }
}

fn storefront_value_title(record: &Value) -> String {
    record
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn storefront_discovery_query_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(|term| {
            term.trim_matches(|character: char| !character.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|term| !term.is_empty())
        .collect()
}

fn storefront_discovery_text_matches(
    texts: &[String],
    query: &str,
    prefix: &str,
    allow_infix: bool,
) -> bool {
    let terms = storefront_discovery_query_terms(query);
    if terms.is_empty() {
        return true;
    }
    let words = texts
        .iter()
        .flat_map(|text| {
            text.split(|character: char| !character.is_alphanumeric())
                .filter(|word| !word.is_empty())
                .map(str::to_ascii_lowercase)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    terms.iter().enumerate().all(|(index, term)| {
        let is_last_prefix = prefix == "LAST" && index + 1 == terms.len();
        words.iter().any(|word| {
            if is_last_prefix {
                word.starts_with(term)
            } else {
                word == term || (allow_infix && word.contains(term))
            }
        })
    })
}

fn storefront_product_matches_discovery_query(
    proxy: &DraftProxy,
    product: &ProductRecord,
    query: &str,
    prefix: &str,
    searchable_fields: &[String],
) -> bool {
    let includes = |field: &str| {
        searchable_fields.is_empty() || searchable_fields.iter().any(|value| value == field)
    };
    let mut texts = Vec::new();
    if includes("TITLE") {
        texts.push(product.title.clone());
    }
    if includes("VENDOR") {
        texts.push(product.vendor.clone());
    }
    if includes("PRODUCT_TYPE") {
        texts.push(product.product_type.clone());
    }
    if includes("TAG") {
        texts.extend(product.tags.clone());
    }
    let variants = proxy.store.product_variants_for_product(&product.id);
    if includes("VARIANT_TITLE") {
        texts.extend(variants.iter().map(|variant| variant.title.clone()));
    }
    if includes("VARIANTS_SKU") {
        texts.extend(variants.iter().map(|variant| variant.sku.clone()));
    }
    if includes("VARIANTS_BARCODE") {
        texts.extend(
            variants
                .iter()
                .filter_map(|variant| variant.barcode.clone()),
        );
    }
    storefront_discovery_text_matches(&texts, query, prefix, true)
}

fn storefront_value_matches_discovery_query(
    record: &Value,
    query: &str,
    prefix: &str,
    searchable_fields: &[String],
) -> bool {
    let includes = |field: &str| {
        searchable_fields.is_empty() || searchable_fields.iter().any(|value| value == field)
    };
    let mut texts = Vec::new();
    if includes("TITLE") {
        if let Some(value) = record.get("title").and_then(Value::as_str) {
            texts.push(value.to_string());
        }
    }
    if includes("BODY") {
        for key in [
            "body",
            "bodySummary",
            "content",
            "contentHtml",
            "summary",
            "excerpt",
        ] {
            if let Some(value) = record.get(key).and_then(Value::as_str) {
                texts.push(value.to_string());
            }
        }
    }
    if includes("TAG") {
        texts.extend(
            record
                .get("tags")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string),
        );
    }
    if includes("AUTHOR") {
        if let Some(value) = record.pointer("/author/name").and_then(Value::as_str) {
            texts.push(value.to_string());
        }
    }
    if searchable_fields.is_empty() {
        if let Some(value) = record.get("handle").and_then(Value::as_str) {
            texts.push(value.to_string());
        }
    }
    storefront_discovery_text_matches(&texts, query, prefix, false)
}

fn storefront_search_product_available(proxy: &DraftProxy, product: &ProductRecord) -> bool {
    let variants = proxy.store.product_variants_for_product(&product.id);
    storefront_product_available_for_sale(product, &variants)
}

fn storefront_product_matches_search_filters(
    proxy: &DraftProxy,
    product: &ProductRecord,
    filters: &[BTreeMap<String, ResolvedValue>],
) -> bool {
    filters.iter().all(|filter| {
        if let Some(available) = resolved_bool_field(filter, "available") {
            if storefront_search_product_available(proxy, product) != available {
                return false;
            }
        }
        if let Some(tag) = resolved_string_field(filter, "tag") {
            if !product
                .tags
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&tag))
            {
                return false;
            }
        }
        if let Some(product_type) = resolved_string_field(filter, "productType") {
            if !product.product_type.eq_ignore_ascii_case(&product_type) {
                return false;
            }
        }
        if let Some(vendor) = resolved_string_field(filter, "productVendor") {
            if !product.vendor.eq_ignore_ascii_case(&vendor) {
                return false;
            }
        }
        let variants = proxy.store.product_variants_for_product(&product.id);
        if let Some(option) = resolved_object_field(filter, "variantOption") {
            let name = resolved_string_field(&option, "name").unwrap_or_default();
            let value = resolved_string_field(&option, "value").unwrap_or_default();
            if !variants.iter().any(|variant| {
                variant.selected_options.iter().any(|option| {
                    option.name.eq_ignore_ascii_case(&name)
                        && option.value.eq_ignore_ascii_case(&value)
                })
            }) {
                return false;
            }
        }
        if let Some(price) = resolved_object_field(filter, "price") {
            let min = price
                .get("min")
                .and_then(resolved_value_number)
                .unwrap_or(0.0);
            let max = price.get("max").and_then(resolved_value_number);
            if !variants.iter().any(|variant| {
                variant
                    .price
                    .parse::<f64>()
                    .ok()
                    .is_some_and(|amount| amount >= min && max.is_none_or(|max| amount <= max))
            }) {
                return false;
            }
        }
        true
    })
}

fn storefront_search_product_filters(
    proxy: &DraftProxy,
    items: &[StorefrontSearchItem],
) -> Vec<Value> {
    let products = items
        .iter()
        .filter_map(|item| match item {
            StorefrontSearchItem::Product(product) => Some(product.as_ref()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if products.is_empty() {
        return vec![json!({
            "id": "filter.v.price", "label": "Price", "presentation": Value::Null, "type": "PRICE_RANGE",
            "values": [{ "id": "filter.v.price", "label": "Price", "count": 0, "input": "{\"price\":{\"min\":0,\"max\":0.0}}" }]
        })];
    }
    let available_count = products
        .iter()
        .filter(|product| storefront_search_product_available(proxy, product))
        .count();
    vec![json!({
        "id": "filter.v.availability", "label": "Availability", "presentation": "TEXT", "type": "LIST",
        "values": [
            { "id": "filter.v.availability.1", "label": "In stock", "count": available_count, "input": "{\"available\":true}" },
            { "id": "filter.v.availability.0", "label": "Out of stock", "count": products.len() - available_count, "input": "{\"available\":false}" }
        ]
    })]
}

fn truncate_with_remaining<T>(values: &mut Vec<T>, remaining: &mut usize) {
    values.truncate(*remaining);
    *remaining = remaining.saturating_sub(values.len());
}

fn storefront_query_suggestions(
    query: &str,
    limit: usize,
    products: &[ProductRecord],
    collections: &[Value],
    articles: &[Value],
    pages: &[Value],
) -> Vec<Value> {
    let normalized = query.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return Vec::new();
    }
    let mut candidates = BTreeSet::new();
    for title in products.iter().map(|record| record.title.as_str()).chain(
        collections
            .iter()
            .chain(articles)
            .chain(pages)
            .filter_map(|record| record.get("title").and_then(Value::as_str)),
    ) {
        for word in title.split(|character: char| !character.is_alphanumeric()) {
            let word = word.to_ascii_lowercase();
            if word.starts_with(&normalized) && word != normalized {
                candidates.insert(word);
            }
        }
    }
    for author in articles
        .iter()
        .filter_map(|record| record.pointer("/author/name").and_then(Value::as_str))
    {
        for word in author.split(|character: char| !character.is_alphanumeric()) {
            let word = word.to_ascii_lowercase();
            if word.starts_with(&normalized) && word != normalized {
                candidates.insert(word);
            }
        }
    }
    let session = format!("{:x}", Sha256::digest(normalized.as_bytes()));
    candidates.into_iter().take(limit).enumerate().map(|(index, text)| {
        let remainder = text.strip_prefix(&normalized).unwrap_or_default();
        json!({
            "text": text,
            "styledText": format!("<mark>{}</mark><span>{}</span>", normalized, remainder),
            "trackingParameters": format!("_pos={}&_psid={}&_psq={}&_ss=e&_v=1.0", index + 1, &session[..9], normalized)
        })
    }).collect()
}

fn storefront_blog_record_from_admin(record: &Value) -> Value {
    json!({
        "__typename": "Blog",
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "handle": record.get("handle").cloned().unwrap_or(Value::Null),
        "title": record.get("title").cloned().unwrap_or(Value::Null),
    })
}

fn storefront_page_record_from_admin(record: &Value) -> Value {
    let body = record
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default();
    json!({
        "__typename": "Page",
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "handle": record.get("handle").cloned().unwrap_or(Value::Null),
        "title": record.get("title").cloned().unwrap_or(Value::Null),
        "body": body,
        "bodySummary": record
            .get("bodySummary")
            .cloned()
            .unwrap_or_else(|| json!(storefront_strip_html(body))),
        "createdAt": record.get("createdAt").cloned().unwrap_or(Value::Null),
        "updatedAt": record.get("updatedAt").cloned().unwrap_or(Value::Null),
    })
}

fn storefront_article_record_from_admin(record: &Value) -> Value {
    let body_html = record
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let excerpt_html = record.get("summary").cloned().unwrap_or(Value::Null);
    json!({
        "__typename": "Article",
        "id": record.get("id").cloned().unwrap_or(Value::Null),
        "blogId": record.get("blogId").cloned().unwrap_or(Value::Null),
        "handle": record.get("handle").cloned().unwrap_or(Value::Null),
        "title": record.get("title").cloned().unwrap_or(Value::Null),
        "content": storefront_strip_html(body_html),
        "contentHtml": body_html,
        "excerpt": excerpt_html
            .as_str()
            .map(storefront_strip_html)
            .map(Value::String)
            .unwrap_or(Value::Null),
        "excerptHtml": excerpt_html,
        "tags": record.get("tags").cloned().unwrap_or_else(|| json!([])),
        "publishedAt": record
            .get("publishedAt")
            .cloned()
            .or_else(|| record.get("createdAt").cloned())
            .unwrap_or(Value::Null),
        "author": storefront_article_author(record.get("author")),
        "image": record.get("image").cloned().unwrap_or(Value::Null),
    })
}

fn storefront_article_author(author: Option<&Value>) -> Value {
    let name = author
        .and_then(|author| author.get("name"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    json!({ "name": name })
}

fn storefront_content_is_visible(record: &Value) -> bool {
    record
        .get("isPublished")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn storefront_default_seo() -> Value {
    json!({
        "title": Value::Null,
        "description": Value::Null,
    })
}

fn storefront_metafields_list(
    arguments: &BTreeMap<String, ResolvedValue>,
    selection: &[SelectedField],
) -> Value {
    let count = match arguments.get("identifiers") {
        Some(ResolvedValue::List(values)) => values.len(),
        _ => 0,
    };
    Value::Array(
        std::iter::repeat_with(|| nullable_selected_json(&Value::Null, selection))
            .take(count)
            .collect(),
    )
}

fn storefront_collection_observed_products(collection: &Value) -> Vec<Value> {
    let mut product_order = Vec::new();
    let mut products = BTreeMap::<String, Value>::new();

    let mut observe_connection = |connection: &Value| {
        for mut product in connection_nodes(connection) {
            let Some(id) = product
                .get("id")
                .and_then(Value::as_str)
                .filter(|id| is_shopify_gid_of_type(id, "Product"))
                .map(str::to_string)
            else {
                continue;
            };
            product["__storefrontVisible"] = json!(true);
            if let Some(existing) = products.remove(&id) {
                products.insert(id, shallow_merged_object(existing, product));
            } else {
                product_order.push(id.clone());
                products.insert(id, product);
            }
        }
    };

    // Preserve the captured default connection prefix first. Other aliases
    // then fill fields and append members that fell outside that window.
    if let Some(default_products) = collection.get("products") {
        observe_connection(default_products);
    }
    if let Some(object) = collection.as_object() {
        for (response_key, value) in object {
            if response_key != "products" {
                observe_connection(value);
            }
        }
    }

    product_order
        .into_iter()
        .filter_map(|id| products.remove(&id))
        .collect()
}

fn storefront_collection_description(collection: &Value, selection: &SelectedField) -> String {
    let description = collection
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            storefront_strip_html(
                collection
                    .get("descriptionHtml")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
        });
    let Some(limit) = resolved_int_field(&selection.arguments, "truncateAt")
        .and_then(|limit| (limit >= 0).then_some(limit as usize))
    else {
        return description;
    };
    if description.chars().count() <= limit {
        return description;
    }
    let prefix_len = limit.saturating_sub(3);
    format!(
        "{}...",
        description.chars().take(prefix_len).collect::<String>()
    )
}

fn storefront_collection_seo(collection: &Value) -> Value {
    collection.get("seo").cloned().unwrap_or_else(|| {
        json!({
            "title": collection.get("title").cloned().unwrap_or(Value::Null),
            "description": storefront_strip_html(
                collection
                    .get("descriptionHtml")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
            )
        })
    })
}

fn storefront_collection_product_matches_filters(
    entry: &CollectionProductEntry,
    filters: &[BTreeMap<String, ResolvedValue>],
) -> bool {
    if filters.is_empty() {
        return true;
    }
    filters.iter().any(|filter| {
        resolved_bool_field(filter, "available").is_none_or(|available| {
            storefront_product_available_for_sale(&entry.product, &entry.variants) == available
        }) && resolved_string_field(filter, "productType").is_none_or(|product_type| {
            entry
                .product
                .product_type
                .eq_ignore_ascii_case(&product_type)
        }) && resolved_string_field(filter, "productVendor")
            .is_none_or(|vendor| entry.product.vendor.eq_ignore_ascii_case(&vendor))
            && resolved_string_field(filter, "tag").is_none_or(|tag| {
                entry
                    .product
                    .tags
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(&tag))
            })
    })
}

fn storefront_truncated_text(value: &str, arguments: &BTreeMap<String, ResolvedValue>) -> String {
    let Some(limit) = resolved_int_field(arguments, "truncateAt")
        .and_then(|limit| (limit >= 0).then_some(limit as usize))
    else {
        return value.to_string();
    };
    value.chars().take(limit).collect()
}

fn storefront_strip_html(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for character in value.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn storefront_content_search_decision(
    kind: StorefrontContentKind,
    record: &Value,
    query: Option<&str>,
) -> StagedSearchDecision {
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return StagedSearchDecision::Match;
    };
    for token in storefront_query_tokens(query) {
        if token.eq_ignore_ascii_case("AND") {
            continue;
        }
        if !storefront_content_matches_token(kind, record, &token) {
            return StagedSearchDecision::NoMatch;
        }
    }
    StagedSearchDecision::Match
}

fn storefront_query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for character in query.chars() {
        match quote {
            Some(active_quote) if character == active_quote => {
                quote = None;
                current.push(character);
            }
            Some(_) => current.push(character),
            None if matches!(character, '"' | '\'') => {
                quote = Some(character);
                current.push(character);
            }
            None if character.is_whitespace() => {
                storefront_push_query_token(&mut tokens, &mut current);
            }
            None => current.push(character),
        }
    }
    storefront_push_query_token(&mut tokens, &mut current);
    tokens
}

fn storefront_push_query_token(tokens: &mut Vec<String>, current: &mut String) {
    let token = current.trim();
    if !token.is_empty() {
        tokens.push(token.to_string());
    }
    current.clear();
}

fn storefront_content_matches_token(
    kind: StorefrontContentKind,
    record: &Value,
    token: &str,
) -> bool {
    let token = token
        .trim()
        .trim_matches(|character: char| matches!(character, '(' | ')' | ','))
        .trim_matches('"')
        .trim_matches('\'');
    let (field, value) = token
        .split_once(':')
        .map(|(field, value)| {
            (
                Some(field.trim().trim_start_matches('-').to_ascii_lowercase()),
                value
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string(),
            )
        })
        .unwrap_or_else(|| (None, token.to_string()));
    let value = value.trim();
    if value.is_empty() {
        return true;
    }
    match field.as_deref() {
        Some("id") => storefront_string_matches(record.get("id").and_then(Value::as_str), value),
        Some("handle") => {
            storefront_string_matches(record.get("handle").and_then(Value::as_str), value)
        }
        Some("title") => {
            storefront_string_matches(record.get("title").and_then(Value::as_str), value)
        }
        Some("author") if kind == StorefrontContentKind::Article => storefront_string_matches(
            record
                .get("author")
                .and_then(|author| author.get("name"))
                .and_then(Value::as_str),
            value,
        ),
        Some("tag") if kind == StorefrontContentKind::Article => record
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .any(|tag| storefront_string_matches(tag.as_str(), value))
            })
            .unwrap_or(false),
        Some("tag_not") if kind == StorefrontContentKind::Article => record
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                !tags
                    .iter()
                    .any(|tag| storefront_string_matches(tag.as_str(), value))
            })
            .unwrap_or(true),
        Some("blog_title") if kind == StorefrontContentKind::Article => false,
        Some("created_at" | "updated_at") => true,
        Some(_) => false,
        None => storefront_content_free_text_matches(kind, record, value),
    }
}

fn storefront_content_free_text_matches(
    kind: StorefrontContentKind,
    record: &Value,
    value: &str,
) -> bool {
    let fields = match kind {
        StorefrontContentKind::Blog => vec!["title", "handle"],
        StorefrontContentKind::Page => vec!["title", "handle", "body", "bodySummary"],
        StorefrontContentKind::Article => vec!["title", "handle", "content", "excerpt"],
    };
    fields
        .iter()
        .any(|field| storefront_string_matches(record.get(*field).and_then(Value::as_str), value))
}

fn storefront_string_matches(actual: Option<&str>, expected: &str) -> bool {
    let expected = expected.trim().to_ascii_lowercase();
    if expected.is_empty() {
        return true;
    }
    let actual = actual.unwrap_or_default().to_ascii_lowercase();
    if let Some(prefix) = expected.strip_suffix('*') {
        return actual
            .split(|character: char| !character.is_ascii_alphanumeric())
            .any(|part| part.starts_with(prefix));
    }
    actual.contains(&expected)
}

fn storefront_content_sort_key(
    kind: StorefrontContentKind,
    record: &Value,
    sort_key: Option<&str>,
) -> StagedSortKey {
    let normalized = sort_key.unwrap_or("ID").to_ascii_uppercase();
    let primary = match normalized.as_str() {
        "TITLE" => storefront_record_sort_string(record, "title"),
        "HANDLE" => storefront_record_sort_string(record, "handle"),
        "AUTHOR" if kind == StorefrontContentKind::Article => record
            .get("author")
            .and_then(|author| author.get("name"))
            .and_then(Value::as_str)
            .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
            .unwrap_or(StagedSortValue::Null),
        "PUBLISHED_AT" if kind == StorefrontContentKind::Article => {
            storefront_record_sort_string(record, "publishedAt")
        }
        "UPDATED_AT" => storefront_record_sort_string(record, "updatedAt"),
        _ => storefront_record_gid_tail_sort_value(record),
    };
    vec![primary, storefront_record_gid_tail_sort_value(record)]
}

fn storefront_record_sort_string(record: &Value, field: &str) -> StagedSortValue {
    record
        .get(field)
        .and_then(Value::as_str)
        .map(|value| StagedSortValue::String(value.to_ascii_lowercase()))
        .unwrap_or(StagedSortValue::Null)
}

fn storefront_record_gid_tail_sort_value(record: &Value) -> StagedSortValue {
    let id = record.get("id").and_then(Value::as_str).unwrap_or_default();
    let tail = resource_id_tail(id);
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| StagedSortValue::String(tail.to_ascii_lowercase()))
}

fn storefront_selected_sitemap_resources(
    resources: &[Value],
    arguments: &BTreeMap<String, ResolvedValue>,
    selection: &[SelectedField],
) -> Value {
    let page = resolved_int_field(arguments, "page")
        .and_then(|page| (page > 0).then_some(page as usize))
        .unwrap_or(1);
    let start = (page - 1) * 250;
    let end = (start + 250).min(resources.len());
    let window = if start < resources.len() {
        &resources[start..end]
    } else {
        &[]
    };
    selected_payload_json(selection, |field| match field.name.as_str() {
        "hasNextPage" => Some(json!(end < resources.len())),
        "items" => Some(Value::Array(
            window
                .iter()
                .map(|resource| selected_json(resource, &field.selection))
                .collect(),
        )),
        _ => None,
    })
}

fn storefront_product_json(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| {
        if !selection_applies_to_type(selection, "Product") {
            return None;
        }
        match selection.name.as_str() {
            "__typename" => Some(json!("Product")),
            "id" => Some(json!(product.id)),
            "title" => Some(json!(product.title)),
            "handle" => Some(json!(product.handle)),
            "createdAt" => Some(json!(product.created_at)),
            "updatedAt" => Some(json!(product.updated_at)),
            "description" => Some(json!(storefront_product_description(product, selection))),
            "descriptionHtml" => Some(json!(product.description_html)),
            "availableForSale" => Some(json!(storefront_product_available_for_sale(
                product, variants
            ))),
            "totalInventory" => Some(json!(storefront_product_total_inventory(product, variants))),
            "vendor" => Some(json!(product.vendor)),
            "productType" => Some(json!(product.product_type)),
            "tags" => Some(json!(product.tags)),
            "publishedAt" => Some(storefront_product_published_at(product)),
            "requiresSellingPlan" => Some(
                product
                    .extra_fields
                    .get("requiresSellingPlan")
                    .cloned()
                    .unwrap_or(Value::Bool(false)),
            ),
            "isGiftCard" => Some(
                product
                    .extra_fields
                    .get("isGiftCard")
                    .cloned()
                    .unwrap_or(Value::Bool(false)),
            ),
            "seo" => Some(product_seo_json(product, &selection.selection)),
            "options" => Some(storefront_product_options_json(
                product, variants, selection,
            )),
            "variants" => Some(storefront_product_variants_connection_json(
                proxy,
                product,
                variants,
                context,
                &selection.arguments,
                &selection.selection,
            )),
            "variantsCount" => Some(selected_count_json(
                storefront_product_variant_count(product, variants),
                &selection.selection,
            )),
            "priceRange" => Some(storefront_product_price_range_json(
                proxy,
                product,
                variants,
                context,
                &selection.selection,
                StorefrontPriceRangeKind::Price,
            )),
            "compareAtPriceRange" => Some(storefront_product_price_range_json(
                proxy,
                product,
                variants,
                context,
                &selection.selection,
                StorefrontPriceRangeKind::CompareAtPrice,
            )),
            "featuredImage" => Some(
                product
                    .media
                    .iter()
                    .find_map(storefront_product_image_json_from_media)
                    .map(|image| selected_json(&image, &selection.selection))
                    .unwrap_or(Value::Null),
            ),
            "images" => Some(storefront_product_images_connection_json(
                product,
                &selection.arguments,
                &selection.selection,
            )),
            "media" => Some(storefront_product_media_connection_json(
                product,
                &selection.arguments,
                &selection.selection,
            )),
            "onlineStoreUrl" => Some(
                product
                    .extra_fields
                    .get("onlineStoreUrl")
                    .cloned()
                    .unwrap_or(Value::Null),
            ),
            "selectedOrFirstAvailableVariant" => {
                Some(storefront_selected_or_first_available_variant_json(
                    proxy, product, variants, context, selection,
                ))
            }
            "variantBySelectedOptions" => Some(storefront_variant_by_selected_options_json(
                proxy, product, variants, context, selection,
            )),
            "metafield" => Some(proxy.storefront_resource_metafield_json(&product.id, selection)),
            "metafields" => Some(proxy.storefront_resource_metafields_json(&product.id, selection)),
            "sellingPlanGroups" => Some(storefront_selling_plan_groups_connection_json(
                proxy, product, variants, context, selection,
            )),
            _ => product
                .extra_fields
                .get(&selection.name)
                .map(|value| nullable_selected_json(value, &selection.selection)),
        }
    })
}

fn storefront_product_description(product: &ProductRecord, selection: &SelectedField) -> String {
    let mut description = product
        .extra_fields
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| strip_html_tags(&product.description_html));
    if let Some(ResolvedValue::Int(limit)) = selection.arguments.get("truncateAt") {
        if *limit >= 0 {
            description = description.chars().take(*limit as usize).collect();
        }
    }
    description
}

fn strip_html_tags(value: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }
    text
}

fn storefront_product_available_for_sale(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> bool {
    if !variants.is_empty() {
        return variants.iter().any(storefront_variant_available_for_sale);
    }
    if !product.variants.is_empty() {
        return product
            .variants
            .iter()
            .any(storefront_raw_variant_available);
    }
    if let Some(observed) = product
        .extra_fields
        .get("availableForSale")
        .and_then(Value::as_bool)
    {
        return observed;
    }
    !product.tracks_inventory || product.total_inventory > 0
}

fn storefront_product_total_inventory(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> i64 {
    if variants.is_empty() {
        return product.total_inventory;
    }
    variants
        .iter()
        .filter(|variant| variant.inventory_item.tracked)
        .map(|variant| variant.inventory_quantity)
        .sum()
}

fn storefront_variant_available_for_sale(variant: &ProductVariantRecord) -> bool {
    !variant.inventory_item.tracked
        || variant.inventory_quantity > 0
        || variant.inventory_policy == "CONTINUE"
}

fn storefront_raw_variant_available(variant: &Value) -> bool {
    variant
        .get("availableForSale")
        .and_then(Value::as_bool)
        .or_else(|| variant.get("available").and_then(Value::as_bool))
        .unwrap_or_else(|| {
            let tracked = variant
                .get("inventoryItem")
                .and_then(|item| item.get("tracked"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let quantity = variant
                .get("inventoryQuantity")
                .or_else(|| variant.get("quantityAvailable"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let policy = variant
                .get("inventoryPolicy")
                .and_then(Value::as_str)
                .unwrap_or_default();
            !tracked || quantity > 0 || policy == "CONTINUE"
        })
}

fn storefront_product_published_at(product: &ProductRecord) -> Value {
    product
        .extra_fields
        .get("publishedAt")
        .cloned()
        .or_else(|| {
            product_publication_entries(product)
                .into_iter()
                .filter_map(|entry| entry.published_at.or(entry.publish_date))
                .min()
                .map(Value::String)
        })
        .unwrap_or(Value::Null)
}

fn storefront_product_variant_count(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> usize {
    if !variants.is_empty() {
        variants.len()
    } else {
        product.variants.len()
    }
}

fn storefront_product_options_json(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    selection: &SelectedField,
) -> Value {
    let mut options = product
        .extra_fields
        .get("options")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_else(|| storefront_options_from_variants(product, variants));
    if let Some(ResolvedValue::Int(first)) = selection.arguments.get("first") {
        if *first >= 0 && options.len() > *first as usize {
            options.truncate(*first as usize);
        }
    }
    Value::Array(
        options
            .iter()
            .map(|option| storefront_product_option_json(option, &selection.selection))
            .collect(),
    )
}

fn storefront_options_from_variants(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
) -> Vec<Value> {
    let mut values_by_name = BTreeMap::<String, Vec<String>>::new();
    for variant in variants {
        for option in &variant.selected_options {
            let values = values_by_name.entry(option.name.clone()).or_default();
            if !values.contains(&option.value) {
                values.push(option.value.clone());
            }
        }
    }
    for variant in &product.variants {
        if let Some(options) = variant.get("selectedOptions").and_then(Value::as_array) {
            for option in options {
                let Some(name) = option.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let Some(value) = option.get("value").and_then(Value::as_str) else {
                    continue;
                };
                let values = values_by_name.entry(name.to_string()).or_default();
                if !values.iter().any(|existing| existing == value) {
                    values.push(value.to_string());
                }
            }
        }
    }
    values_by_name
        .into_iter()
        .enumerate()
        .map(|(index, (name, values))| {
            json!({
                "id": format!("{}/options/{}", product.id, index + 1),
                "name": name,
                "values": values,
                "optionValues": values
                    .iter()
                    .map(|value| json!({ "id": Value::Null, "name": value, "swatch": Value::Null }))
                    .collect::<Vec<_>>()
            })
        })
        .collect()
}

fn storefront_product_option_json(option: &Value, selections: &[SelectedField]) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("ProductOption")),
        "id" => option.get("id").cloned(),
        "name" => option.get("name").cloned(),
        "values" => option.get("values").cloned().or_else(|| {
            option
                .get("optionValues")
                .and_then(Value::as_array)
                .map(|values| {
                    Value::Array(
                        values
                            .iter()
                            .filter_map(|value| value.get("name").cloned())
                            .collect(),
                    )
                })
        }),
        "optionValues" => option.get("optionValues").cloned().map(|values| {
            if let Some(values) = values.as_array() {
                Value::Array(
                    values
                        .iter()
                        .map(|value| selected_json(value, &selection.selection))
                        .collect(),
                )
            } else {
                Value::Array(Vec::new())
            }
        }),
        _ => None,
    })
}

fn storefront_product_variants_connection_json(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    if variants.is_empty() {
        let raw_variants = sorted_storefront_raw_variants(product.variants.clone(), arguments);
        return selected_typed_connection_with_args(
            &raw_variants,
            arguments,
            selections,
            selected_json,
            value_id_cursor,
        );
    }
    let variants = sorted_storefront_variants(variants.to_vec(), arguments);
    selected_typed_connection_with_args(
        &variants,
        arguments,
        selections,
        |variant, selections| {
            storefront_product_variant_json(
                proxy,
                variant,
                Some(product),
                context,
                None,
                selections,
            )
        },
        |variant| variant.id.clone(),
    )
}

fn sorted_storefront_variants(
    variants: Vec<ProductVariantRecord>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<ProductVariantRecord> {
    let sort_key_name = resolved_string_field(arguments, "sortKey");
    sorted_indexed_records(
        variants,
        resolved_bool_field(arguments, "reverse").unwrap_or(false),
        |variant, index| storefront_variant_sort_key(variant, sort_key_name.as_deref(), index),
        |variant| variant.id.clone(),
    )
}

fn sorted_storefront_raw_variants(
    variants: Vec<Value>,
    arguments: &BTreeMap<String, ResolvedValue>,
) -> Vec<Value> {
    let sort_key_name = resolved_string_field(arguments, "sortKey");
    sorted_indexed_records(
        variants,
        resolved_bool_field(arguments, "reverse").unwrap_or(false),
        |variant, index| storefront_raw_variant_sort_key(variant, sort_key_name.as_deref(), index),
        value_id_cursor,
    )
}

fn storefront_variant_sort_key(
    variant: &ProductVariantRecord,
    sort_key: Option<&str>,
    index: usize,
) -> StagedSortKey {
    match sort_key {
        Some("ID") => storefront_gid_sort_key(&variant.id),
        Some("SKU") => vec![storefront_sort_string(&variant.sku)],
        Some("TITLE") => vec![storefront_sort_string(&variant.title)],
        Some("POSITION") | Some("RELEVANCE") | None => vec![StagedSortValue::I64(
            product_variant_position(variant).unwrap_or(index as i64),
        )],
        _ => vec![StagedSortValue::I64(index as i64)],
    }
}

fn storefront_raw_variant_sort_key(
    variant: &Value,
    sort_key: Option<&str>,
    index: usize,
) -> StagedSortKey {
    match sort_key {
        Some("ID") => variant
            .get("id")
            .and_then(Value::as_str)
            .map(storefront_gid_sort_key)
            .unwrap_or_else(|| vec![StagedSortValue::Null]),
        Some("SKU") => vec![storefront_sort_string(
            variant
                .get("sku")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )],
        Some("TITLE") => vec![storefront_sort_string(
            variant
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )],
        Some("POSITION") | Some("RELEVANCE") | None => {
            let position = variant
                .get("position")
                .and_then(Value::as_i64)
                .unwrap_or(index as i64);
            vec![StagedSortValue::I64(position)]
        }
        _ => vec![StagedSortValue::I64(index as i64)],
    }
}

pub(in crate::proxy) fn storefront_product_variant_json(
    proxy: &DraftProxy,
    variant: &ProductVariantRecord,
    product: Option<&ProductRecord>,
    context: &StorefrontRequestContext,
    currency_code_override: Option<&str>,
    selections: &[SelectedField],
) -> Value {
    let mut pricing = proxy.storefront_variant_pricing(variant, context);
    if let Some(currency_code) = currency_code_override {
        pricing.currency_code = currency_code.to_string();
    }
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("ProductVariant")),
        "id" => Some(json!(variant.id)),
        "title" => Some(json!(variant.title)),
        "sku" => Some(if variant.sku.is_empty() {
            Value::Null
        } else {
            json!(variant.sku)
        }),
        "barcode" => Some(
            variant
                .barcode
                .as_ref()
                .map(|value| json!(value))
                .unwrap_or(Value::Null),
        ),
        "availableForSale" => Some(json!(storefront_variant_available_for_sale(variant))),
        "currentlyNotInStock" => Some(json!(
            variant.inventory_item.tracked
                && variant.inventory_quantity <= 0
                && variant.inventory_policy == "CONTINUE"
        )),
        "quantityAvailable" => Some(if variant.inventory_item.tracked {
            json!(variant.inventory_quantity.max(0))
        } else {
            Value::Null
        }),
        "requiresShipping" => Some(json!(variant.inventory_item.requires_shipping)),
        "taxable" => Some(json!(variant.taxable)),
        "price" | "priceV2" => Some(storefront_money_json(
            &pricing.price,
            &pricing.currency_code,
            &selection.selection,
        )),
        "compareAtPrice" | "compareAtPriceV2" => Some(
            pricing
                .compare_at_price
                .as_ref()
                .map(|price| {
                    storefront_money_json(price, &pricing.currency_code, &selection.selection)
                })
                .unwrap_or(Value::Null),
        ),
        "selectedOptions" => Some(Value::Array(
            variant
                .selected_options
                .iter()
                .map(|option| {
                    selected_json(
                        &json!({ "name": option.name, "value": option.value }),
                        &selection.selection,
                    )
                })
                .collect(),
        )),
        "image" => Some(
            product
                .and_then(|product| {
                    variant
                        .media_ids
                        .iter()
                        .find_map(|media_id| {
                            product
                                .media
                                .iter()
                                .find(|media| {
                                    media.get("id").and_then(Value::as_str)
                                        == Some(media_id.as_str())
                                })
                                .and_then(storefront_product_image_json_from_media)
                        })
                        .or_else(|| {
                            product
                                .media
                                .iter()
                                .find_map(storefront_product_image_json_from_media)
                        })
                })
                .map(|image| selected_json(&image, &selection.selection))
                .unwrap_or(Value::Null),
        ),
        "product" => Some(match product {
            Some(product) => {
                storefront_product_json(proxy, product, &[], context, &selection.selection)
            }
            None => Value::Null,
        }),
        "unitPrice" | "unitPriceMeasurement" | "shopPayInstallmentsPricing" => Some(Value::Null),
        "metafield" => Some(proxy.storefront_resource_metafield_json(&variant.id, selection)),
        "metafields" => Some(proxy.storefront_resource_metafields_json(&variant.id, selection)),
        "sellingPlanAllocations" => Some(storefront_selling_plan_allocations_connection_json(
            proxy, variant, product, context, selection,
        )),
        "components" | "groupedBy" | "quantityPriceBreaks" | "storeAvailability" => {
            Some(selected_empty_connection_json(&selection.selection))
        }
        "quantityRule" => Some(selected_json(
            &json!({ "increment": 1, "maximum": Value::Null, "minimum": 1 }),
            &selection.selection,
        )),
        "requiresComponents" => Some(Value::Bool(false)),
        "weight" => variant
            .extra_fields
            .get("weight")
            .cloned()
            .or(Some(Value::Null)),
        "weightUnit" => variant
            .extra_fields
            .get("weightUnit")
            .cloned()
            .or_else(|| Some(json!("KILOGRAMS"))),
        _ => variant
            .extra_fields
            .get(&selection.name)
            .map(|value| nullable_selected_json(value, &selection.selection)),
    })
}

fn storefront_selected_or_first_available_variant_json(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
    selection: &SelectedField,
) -> Value {
    let selected = storefront_variant_matching_selected_options(variants, selection)
        .or_else(|| {
            variants
                .iter()
                .find(|variant| storefront_variant_available_for_sale(variant))
        })
        .or_else(|| variants.first());
    selected
        .map(|variant| {
            storefront_product_variant_json(
                proxy,
                variant,
                Some(product),
                context,
                None,
                &selection.selection,
            )
        })
        .unwrap_or(Value::Null)
}

fn storefront_variant_by_selected_options_json(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
    selection: &SelectedField,
) -> Value {
    storefront_variant_matching_selected_options(variants, selection)
        .map(|variant| {
            storefront_product_variant_json(
                proxy,
                variant,
                Some(product),
                context,
                None,
                &selection.selection,
            )
        })
        .unwrap_or(Value::Null)
}

fn storefront_variant_matching_selected_options<'a>(
    variants: &'a [ProductVariantRecord],
    selection: &SelectedField,
) -> Option<&'a ProductVariantRecord> {
    let selected = resolved_object_list_field(&selection.arguments, "selectedOptions")
        .into_iter()
        .filter_map(|option| {
            Some((
                resolved_string_field(&option, "name")?,
                resolved_string_field(&option, "value")?,
            ))
        })
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return None;
    }
    let case_insensitive =
        resolved_bool_field(&selection.arguments, "caseInsensitiveMatch").unwrap_or(false);
    variants.iter().find(|variant| {
        selected.iter().all(|(name, value)| {
            variant.selected_options.iter().any(|option| {
                if case_insensitive {
                    option.name.eq_ignore_ascii_case(name)
                        && option.value.eq_ignore_ascii_case(value)
                } else {
                    option.name == *name && option.value == *value
                }
            })
        })
    })
}

#[derive(Clone, Copy)]
enum StorefrontPriceRangeKind {
    Price,
    CompareAtPrice,
}

fn storefront_product_price_range_json(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
    selections: &[SelectedField],
    kind: StorefrontPriceRangeKind,
) -> Value {
    let observed_field = match kind {
        StorefrontPriceRangeKind::Price => "priceRange",
        StorefrontPriceRangeKind::CompareAtPrice => "compareAtPriceRange",
    };
    if variants.is_empty() && product.variants.is_empty() {
        if let Some(observed) = product.extra_fields.get(observed_field) {
            return selected_json(observed, selections);
        }
    }
    let prices = match kind {
        StorefrontPriceRangeKind::Price => {
            storefront_product_variant_prices(proxy, product, variants, context)
        }
        StorefrontPriceRangeKind::CompareAtPrice => {
            storefront_product_variant_compare_at_prices(proxy, product, variants, context)
        }
    };
    let (min_price, max_price) = storefront_price_bounds(prices).unwrap_or((0.0, 0.0));
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("ProductPriceRange")),
        "minVariantPrice" => Some(storefront_money_json(
            &format!("{min_price:.2}"),
            &storefront_product_currency_code(proxy, variants, context),
            &selection.selection,
        )),
        "maxVariantPrice" => Some(storefront_money_json(
            &format!("{max_price:.2}"),
            &storefront_product_currency_code(proxy, variants, context),
            &selection.selection,
        )),
        _ => None,
    })
}

fn storefront_product_variant_prices(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
) -> Vec<f64> {
    if !variants.is_empty() {
        return variants
            .iter()
            .filter_map(|variant| {
                storefront_parse_price(&proxy.storefront_variant_pricing(variant, context).price)
            })
            .collect();
    }
    product
        .variants
        .iter()
        .filter_map(|variant| variant.get("price").and_then(Value::as_str))
        .filter_map(storefront_parse_price)
        .collect()
}

fn storefront_product_variant_compare_at_prices(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
) -> Vec<f64> {
    if !variants.is_empty() {
        return variants
            .iter()
            .filter_map(|variant| {
                proxy
                    .storefront_variant_pricing(variant, context)
                    .compare_at_price
            })
            .filter_map(|price| storefront_parse_price(&price))
            .collect();
    }
    product
        .variants
        .iter()
        .filter_map(|variant| variant.get("compareAtPrice").and_then(Value::as_str))
        .filter_map(storefront_parse_price)
        .collect()
}

fn storefront_price_bounds(prices: Vec<f64>) -> Option<(f64, f64)> {
    let mut iter = prices.into_iter();
    let first = iter.next()?;
    let mut min_price = first;
    let mut max_price = first;
    for price in iter {
        if price < min_price {
            min_price = price;
        }
        if price > max_price {
            max_price = price;
        }
    }
    Some((min_price, max_price))
}

fn storefront_parse_price(price: &str) -> Option<f64> {
    price.trim().parse::<f64>().ok()
}

fn storefront_product_currency_code(
    proxy: &DraftProxy,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
) -> String {
    variants
        .first()
        .map(|variant| {
            proxy
                .storefront_variant_pricing(variant, context)
                .currency_code
        })
        .filter(|currency| !currency.is_empty())
        .or_else(|| {
            proxy
                .storefront_context_localization(context)
                .and_then(|localization| localization.pointer("/country/currency/isoCode"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| proxy.store.observed_shop_currency_code())
        .unwrap_or_default()
}

fn storefront_selling_plan_groups_connection_json(
    proxy: &DraftProxy,
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    context: &StorefrontRequestContext,
    selection: &SelectedField,
) -> Value {
    let variant_ids = variants
        .iter()
        .map(|variant| variant.id.as_str())
        .collect::<BTreeSet<_>>();
    let groups = proxy
        .store
        .selling_plan_groups()
        .into_iter()
        .filter(|group| {
            group.product_ids.iter().any(|id| id == &product.id)
                || group
                    .product_variant_ids
                    .iter()
                    .any(|id| variant_ids.contains(id.as_str()))
        })
        .collect::<Vec<_>>();
    let currency_code = storefront_product_currency_code(proxy, variants, context);
    selected_typed_connection_with_args(
        &groups,
        &selection.arguments,
        &selection.selection,
        |group, selections| storefront_selling_plan_group_json(group, &currency_code, selections),
        |group| group.id.clone(),
    )
}

fn storefront_selling_plan_group_json(
    group: &SellingPlanGroupRecord,
    currency_code: &str,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("SellingPlanGroup")),
        "appName" => Some(Value::Null),
        "name" => Some(json!(group.name)),
        "options" => Some(Value::Array(
            group
                .options
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    let mut values = group
                        .selling_plans
                        .iter()
                        .filter_map(|plan| plan.options.get(index).cloned())
                        .collect::<Vec<_>>();
                    values.dedup();
                    selected_json(
                        &json!({ "name": name, "values": values }),
                        &selection.selection,
                    )
                })
                .collect(),
        )),
        "sellingPlans" => Some(selected_typed_connection_with_args(
            &group.selling_plans,
            &selection.arguments,
            &selection.selection,
            |plan, selections| {
                storefront_selling_plan_json(plan, &group.options, currency_code, selections)
            },
            |plan| plan.id.clone(),
        )),
        _ => None,
    })
}

fn storefront_selling_plan_json(
    plan: &SellingPlanRecord,
    option_names: &[String],
    currency_code: &str,
    selections: &[SelectedField],
) -> Value {
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("SellingPlan")),
        "id" => Some(json!(plan.id)),
        "name" => Some(json!(plan.name)),
        "description" => Some(json!(plan.description)),
        "recurringDeliveries" => Some(json!(
            plan.delivery_policy
                .get("__typename")
                .and_then(Value::as_str)
                == Some("SellingPlanRecurringDeliveryPolicy")
        )),
        "options" => Some(Value::Array(
            plan.options
                .iter()
                .enumerate()
                .map(|(index, value)| {
                    selected_json(
                        &json!({
                            "name": option_names.get(index).cloned().unwrap_or_default(),
                            "value": value
                        }),
                        &selection.selection,
                    )
                })
                .collect(),
        )),
        "priceAdjustments" => Some(Value::Array(
            plan.pricing_policies
                .iter()
                .map(|policy| {
                    selected_payload_json(&selection.selection, |field| match field.name.as_str() {
                        "orderCount" => Some(
                            policy
                                .get("afterCycle")
                                .and_then(Value::as_i64)
                                .map(|after_cycle| json!(after_cycle + 1))
                                .unwrap_or(Value::Null),
                        ),
                        "adjustmentValue" => Some(storefront_selling_plan_adjustment_value_json(
                            policy,
                            currency_code,
                            &field.selection,
                        )),
                        _ => None,
                    })
                })
                .collect(),
        )),
        _ => None,
    })
}

fn storefront_selling_plan_adjustment_value_json(
    policy: &Value,
    currency_code: &str,
    selections: &[SelectedField],
) -> Value {
    let adjustment_type = policy
        .get("adjustmentType")
        .and_then(Value::as_str)
        .unwrap_or("PERCENTAGE");
    selected_payload_json(selections, |selection| {
        match (adjustment_type, selection.name.as_str()) {
            (_, "__typename") => Some(json!(match adjustment_type {
                "FIXED_AMOUNT" => "SellingPlanFixedAmountPriceAdjustment",
                "PRICE" => "SellingPlanFixedPriceAdjustment",
                _ => "SellingPlanPercentagePriceAdjustment",
            })),
            ("FIXED_AMOUNT", "adjustmentAmount") => Some(storefront_money_json(
                policy
                    .pointer("/adjustmentValue/amount")
                    .and_then(Value::as_str)
                    .unwrap_or("0"),
                currency_code,
                &selection.selection,
            )),
            ("PRICE", "price") => Some(storefront_money_json(
                policy
                    .pointer("/adjustmentValue/amount")
                    .and_then(Value::as_str)
                    .unwrap_or("0"),
                currency_code,
                &selection.selection,
            )),
            (_, "adjustmentPercentage") => Some(
                policy
                    .pointer("/adjustmentValue/percentage")
                    .cloned()
                    .unwrap_or_else(|| json!(0)),
            ),
            _ => None,
        }
    })
}

fn storefront_selling_plan_allocations_connection_json(
    proxy: &DraftProxy,
    variant: &ProductVariantRecord,
    product: Option<&ProductRecord>,
    context: &StorefrontRequestContext,
    selection: &SelectedField,
) -> Value {
    let Some(product) = product else {
        return selected_empty_connection_json(&selection.selection);
    };
    let allocations = proxy
        .store
        .selling_plan_groups()
        .into_iter()
        .filter(|group| {
            group.product_ids.iter().any(|id| id == &product.id)
                || group.product_variant_ids.iter().any(|id| id == &variant.id)
        })
        .flat_map(|group| group.selling_plans)
        .collect::<Vec<_>>();
    let pricing = proxy.storefront_variant_pricing(variant, context);
    selected_typed_connection_with_args(
        &allocations,
        &selection.arguments,
        &selection.selection,
        |plan, selections| storefront_selling_plan_allocation_json(plan, &pricing, selections),
        |plan| plan.id.clone(),
    )
}

fn storefront_selling_plan_allocation_json(
    plan: &SellingPlanRecord,
    pricing: &StorefrontVariantPricing,
    selections: &[SelectedField],
) -> Value {
    let original = storefront_parse_price(&pricing.price).unwrap_or_default();
    let adjusted = plan
        .pricing_policies
        .first()
        .map(|policy| storefront_adjusted_selling_plan_price(original, policy))
        .unwrap_or(original);
    let adjusted_amount = format_money_amount(adjusted);
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "checkoutChargeAmount" => Some(storefront_money_json(
            &adjusted_amount,
            &pricing.currency_code,
            &selection.selection,
        )),
        "remainingBalanceChargeAmount" => Some(storefront_money_json(
            "0",
            &pricing.currency_code,
            &selection.selection,
        )),
        "priceAdjustments" => Some(Value::Array(vec![selected_payload_json(
            &selection.selection,
            |field| match field.name.as_str() {
                "price" | "perDeliveryPrice" => Some(storefront_money_json(
                    &adjusted_amount,
                    &pricing.currency_code,
                    &field.selection,
                )),
                "compareAtPrice" => Some(storefront_money_json(
                    &pricing.price,
                    &pricing.currency_code,
                    &field.selection,
                )),
                "unitPrice" => Some(Value::Null),
                _ => None,
            },
        )])),
        "sellingPlan" => Some(storefront_selling_plan_json(
            plan,
            &[],
            &pricing.currency_code,
            &selection.selection,
        )),
        _ => None,
    })
}

fn storefront_adjusted_selling_plan_price(price: f64, policy: &Value) -> f64 {
    let value = policy
        .pointer("/adjustmentValue/percentage")
        .and_then(Value::as_f64)
        .or_else(|| {
            policy
                .pointer("/adjustmentValue/percentage")
                .and_then(Value::as_i64)
                .map(|value| value as f64)
        })
        .unwrap_or_default();
    match policy
        .get("adjustmentType")
        .and_then(Value::as_str)
        .unwrap_or("PERCENTAGE")
    {
        "FIXED_AMOUNT" => {
            let amount = policy
                .pointer("/adjustmentValue/amount")
                .and_then(Value::as_str)
                .and_then(storefront_parse_price)
                .unwrap_or_default();
            (price - amount).max(0.0)
        }
        "PRICE" => policy
            .pointer("/adjustmentValue/amount")
            .and_then(Value::as_str)
            .and_then(storefront_parse_price)
            .unwrap_or(price),
        _ => price * (1.0 - value / 100.0),
    }
}

pub(in crate::proxy) fn storefront_money_json(
    price: &str,
    currency_code: &str,
    selections: &[SelectedField],
) -> Value {
    let amount = normalize_money_amount(price);
    selected_payload_json(selections, |selection| match selection.name.as_str() {
        "__typename" => Some(json!("MoneyV2")),
        "amount" => Some(json!(amount)),
        "currencyCode" => Some(json!(currency_code)),
        _ => None,
    })
}

fn storefront_product_images_connection_json(
    product: &ProductRecord,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let images = product
        .media
        .iter()
        .filter_map(storefront_product_image_json_from_media)
        .collect::<Vec<_>>();
    selected_connection_json_with_args(images, arguments, selections, value_id_cursor)
}

fn storefront_product_media_connection_json(
    product: &ProductRecord,
    arguments: &BTreeMap<String, ResolvedValue>,
    selections: &[SelectedField],
) -> Value {
    let media = product
        .media
        .iter()
        .map(storefront_product_media_json)
        .collect::<Vec<_>>();
    selected_connection_json_with_args(media, arguments, selections, value_id_cursor)
}

fn storefront_product_media_json(media: &Value) -> Value {
    let image = storefront_media_image_json(media);
    let mut value = json!({
        "__typename": media
            .get("__typename")
            .cloned()
            .unwrap_or_else(|| json!("MediaImage")),
        "id": media.get("id").cloned().unwrap_or(Value::Null),
        "alt": media.get("alt").cloned().unwrap_or(Value::Null),
        "mediaContentType": media
            .get("mediaContentType")
            .cloned()
            .unwrap_or_else(|| json!("IMAGE")),
        "previewImage": image.clone(),
    });
    if media.get("mediaContentType").and_then(Value::as_str) == Some("IMAGE")
        || media.get("__typename").and_then(Value::as_str) == Some("MediaImage")
    {
        value["image"] = image;
    }
    value
}

fn storefront_media_image_json(media: &Value) -> Value {
    let Some(mut image) = storefront_product_image_json_from_media(media) else {
        return Value::Null;
    };
    if let Some(media_id) = media.get("id").and_then(Value::as_str) {
        image["id"] = json!(shopify_gid("ImageSource", resource_id_tail(media_id)));
    }
    if image.get("width").is_none_or(Value::is_null)
        || image.get("height").is_none_or(Value::is_null)
    {
        if let Some(source) = image.get("url").and_then(Value::as_str) {
            if let Some((width, height)) = storefront_image_dimensions_from_url(source) {
                image["width"] = json!(width);
                image["height"] = json!(height);
            }
        }
    }
    image
}

fn storefront_product_image_json_from_media(media: &Value) -> Option<Value> {
    let mut image = product_image_json_from_media(media).or_else(|| {
        let source = media
            .pointer("/originalSource/url")
            .and_then(Value::as_str)
            .or_else(|| media.get("originalSource").and_then(Value::as_str))?;
        let media_id = media.get("id").and_then(Value::as_str)?;
        Some(json!({
            "id": shopify_gid("ProductImage", resource_id_tail(media_id)),
            "url": source,
            "altText": media.get("alt").cloned().unwrap_or(Value::Null),
            "width": Value::Null,
            "height": Value::Null
        }))
    })?;
    image.as_object_mut()?.remove("__typename");
    if image.get("width").is_none_or(Value::is_null)
        || image.get("height").is_none_or(Value::is_null)
    {
        if let Some(source) = image.get("url").and_then(Value::as_str) {
            if let Some((width, height)) = storefront_image_dimensions_from_url(source) {
                image["width"] = json!(width);
                image["height"] = json!(height);
            }
        }
    }
    Some(image)
}

fn storefront_image_dimensions_from_url(url: &str) -> Option<(i64, i64)> {
    url.split(['/', '?', '&']).find_map(|part| {
        let (width, height) = part.split_once('x')?;
        let width = width.parse::<i64>().ok()?;
        let height = height.parse::<i64>().ok()?;
        (width > 0 && height > 0).then_some((width, height))
    })
}

fn storefront_product_sort_key(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    sort_key: Option<&str>,
) -> StagedSortKey {
    let primary = match sort_key {
        Some("TITLE") => storefront_sort_string(&product.title),
        Some("PRODUCT_TYPE") => storefront_sort_string(&product.product_type),
        Some("VENDOR") => storefront_sort_string(&product.vendor),
        Some("UPDATED_AT") => StagedSortValue::String(product.updated_at.clone()),
        None | Some("CREATED_AT") | Some("BEST_SELLING") | Some("RELEVANCE") => {
            StagedSortValue::String(product.created_at.clone())
        }
        Some("PRICE") => {
            let prices = if variants.is_empty() {
                product
                    .variants
                    .iter()
                    .filter_map(|variant| variant.get("price").and_then(Value::as_str))
                    .filter_map(storefront_parse_price)
                    .collect::<Vec<_>>()
            } else {
                variants
                    .iter()
                    .filter_map(|variant| storefront_parse_price(&variant.price))
                    .collect::<Vec<_>>()
            };
            storefront_price_bounds(prices)
                .map(|(min_price, _)| StagedSortValue::String(format!("{min_price:020.4}")))
                .unwrap_or(StagedSortValue::Null)
        }
        Some("ID") => return storefront_gid_sort_key(&product.id),
        Some(_) => storefront_gid_sort_key(&product.id)
            .into_iter()
            .next()
            .unwrap_or(StagedSortValue::Null),
    };
    vec![primary, storefront_gid_tail_sort_value(&product.id)]
}

fn storefront_gid_sort_key(id: &str) -> StagedSortKey {
    vec![storefront_gid_tail_sort_value(id)]
}

fn storefront_gid_tail_sort_value(id: &str) -> StagedSortValue {
    let tail = resource_id_tail(id);
    tail.parse::<i64>()
        .map(StagedSortValue::I64)
        .unwrap_or_else(|_| storefront_sort_string(tail))
}

fn storefront_sort_string(value: impl AsRef<str>) -> StagedSortValue {
    StagedSortValue::String(value.as_ref().to_ascii_lowercase())
}

pub(in crate::proxy) fn storefront_request_context(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> StorefrontRequestContext {
    let mut context = StorefrontRequestContext::default();
    let Ok(directives) = operation_directive_invocations(query, variables, None) else {
        return context;
    };
    for directive in directives
        .into_iter()
        .filter(|directive| directive.name == "inContext")
    {
        if let Some(country) = resolved_value_string(directive.arguments.get("country")) {
            context.country = Some(country);
        }
        if let Some(language) = resolved_value_string(directive.arguments.get("language")) {
            context.language = Some(language);
        }
        if let Some(preferred_location_id) =
            resolved_value_string(directive.arguments.get("preferredLocationId"))
        {
            context.preferred_location_id = Some(preferred_location_id);
        }
        if let Some(ResolvedValue::Object(buyer)) = directive.arguments.get("buyer") {
            context.buyer_customer_access_token =
                resolved_string_field(buyer, "customerAccessToken");
            context.buyer_company_location_id = resolved_string_field(buyer, "companyLocationId");
        }
        context.uses_enrichment_context = directive.arguments.contains_key("preferredLocationId")
            || directive.arguments.contains_key("buyer");
    }
    context
}

fn storefront_first_slice_hydrate_body(
    context: &StorefrontRequestContext,
) -> (&'static str, Value) {
    if context.uses_enrichment_context {
        return (
            STOREFRONT_ENRICHMENT_CONTEXT_HYDRATE_QUERY,
            json!({
                "country": context.country,
                "language": context.language,
                // A locally allocated Admin Location id cannot be sent to Shopify's
                // Storefront API. Captured behavior shows it does not alter the
                // country/language localization or this scenario's empty availability.
                "preferredLocationId": Value::Null,
                "buyer": Value::Null
            }),
        );
    }
    if context.has_in_context_values() {
        (
            STOREFRONT_FIRST_SLICE_CONTEXT_HYDRATE_QUERY,
            json!({
                "country": context.country,
                "language": context.language
            }),
        )
    } else {
        (STOREFRONT_FIRST_SLICE_HYDRATE_QUERY, json!({}))
    }
}

fn resolved_value_string(value: Option<&ResolvedValue>) -> Option<String> {
    match value {
        Some(ResolvedValue::String(value)) if !value.is_empty() => Some(value.clone()),
        _ => None,
    }
}

fn selection_applies_to_type(selection: &SelectedField, type_name: &str) -> bool {
    match selection.type_condition.as_deref() {
        None => true,
        Some("Node") => true,
        Some("SearchResultItem") => matches!(type_name, "Article" | "Page" | "Product"),
        Some("HasMetafields") => matches!(
            type_name,
            "Article"
                | "Blog"
                | "Collection"
                | "Customer"
                | "Page"
                | "Product"
                | "ProductVariant"
                | "Shop"
        ),
        Some("OnlineStorePublishable") => type_name == "Metaobject",
        Some("MetafieldReference") => matches!(
            type_name,
            "Article"
                | "Collection"
                | "GenericFile"
                | "MediaImage"
                | "Metaobject"
                | "Model3d"
                | "Page"
                | "Product"
                | "ProductVariant"
                | "Video"
        ),
        Some("MetafieldParentResource") => matches!(
            type_name,
            "Article"
                | "Blog"
                | "Cart"
                | "Collection"
                | "Company"
                | "CompanyLocation"
                | "Customer"
                | "Location"
                | "Market"
                | "Order"
                | "Page"
                | "Product"
                | "ProductVariant"
                | "SellingPlan"
                | "Shop"
        ),
        Some(condition) => condition == type_name,
    }
}

fn storefront_metafield_is_public(metafield: &Value) -> bool {
    metafield
        .pointer("/definition/access/storefront")
        .and_then(Value::as_str)
        == Some("PUBLIC_READ")
        || metafield
            .get("__storefrontPublic")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn storefront_metaobject_fields(record: &Value) -> Value {
    let mut fields = record["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    fields.sort_by(|left, right| {
        left.get("key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(right.get("key").and_then(Value::as_str).unwrap_or_default())
    });
    Value::Array(fields)
}

fn storefront_metaobject_sort_key(record: &Value, sort_key: Option<&str>) -> StagedSortKey {
    let sort_key = sort_key
        .unwrap_or("id")
        .replace('-', "_")
        .to_ascii_lowercase();
    let primary = match sort_key.as_str() {
        "updated_at" | "updatedat" => StagedSortValue::String(
            record
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        ),
        _ => resource_id_tail_sort_value(record.get("id").and_then(Value::as_str)),
    };
    vec![
        primary,
        resource_id_tail_sort_value(record.get("id").and_then(Value::as_str)),
    ]
}

fn storefront_policy_from_admin(policy: &ShopPolicyRecord) -> Value {
    let mut policy = shop_policy_record_json(policy);
    policy["handle"] = policy
        .get("handle")
        .cloned()
        .or_else(|| {
            policy
                .get("url")
                .and_then(Value::as_str)
                .and_then(policy_handle_from_url)
                .map(Value::String)
        })
        .unwrap_or(Value::Null);
    policy
}

fn policy_handle_from_url(url: &str) -> Option<String> {
    let without_query = url.split('?').next().unwrap_or(url);
    let segment = without_query
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    Some(segment.strip_suffix(".html").unwrap_or(segment).to_string())
}

fn push_storefront_location(
    records: &mut Vec<Value>,
    seen: &mut BTreeSet<String>,
    location: Value,
) {
    let Some(id) = location.get("id").and_then(Value::as_str) else {
        return;
    };
    if seen.insert(id.to_string()) {
        records.push(location);
    }
}

fn push_admin_location_as_storefront(
    records: &mut Vec<Value>,
    seen: &mut BTreeSet<String>,
    location: &Value,
) {
    if location.get("isActive").and_then(Value::as_bool) == Some(false)
        || location
            .get("isFulfillmentService")
            .and_then(Value::as_bool)
            == Some(true)
    {
        return;
    }
    push_storefront_location(records, seen, storefront_location_from_admin(location));
}

fn storefront_location_from_admin(location: &Value) -> Value {
    json!({
        "id": location.get("id").cloned().unwrap_or(Value::Null),
        "name": location.get("name").cloned().unwrap_or(Value::Null),
        "address": location.get("address").cloned().unwrap_or(Value::Null)
    })
}

fn sort_storefront_locations(records: &mut [Value], arguments: &BTreeMap<String, ResolvedValue>) {
    let sort_key = resolved_string_field(arguments, "sortKey").unwrap_or_else(|| "ID".to_string());
    records.sort_by(|left, right| {
        storefront_location_sort_value(left, &sort_key)
            .cmp(&storefront_location_sort_value(right, &sort_key))
            .then_with(|| {
                left.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(right.get("id").and_then(Value::as_str).unwrap_or_default())
            })
    });
    if resolved_bool_field(arguments, "reverse").unwrap_or(false) {
        records.reverse();
    }
}

fn storefront_location_sort_value(location: &Value, sort_key: &str) -> String {
    match sort_key {
        "NAME" => location
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        "CITY" => location
            .pointer("/address/city")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        _ => location
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    }
}

fn storefront_location_cursor(location: &Value, cursor_by_id: &BTreeMap<String, String>) -> String {
    location
        .get("id")
        .and_then(Value::as_str)
        .and_then(|id| cursor_by_id.get(id).cloned())
        .unwrap_or_default()
}

fn storefront_customer_shared_record(
    id: &str,
    first_name: Option<&str>,
    last_name: Option<&str>,
    email: &str,
    phone: Option<&str>,
    accepts_marketing: bool,
    timestamp: &str,
) -> Value {
    let display_name = storefront_customer_display_name(first_name, last_name, Some(email));
    json!({
        "id": id,
        "firstName": first_name,
        "lastName": last_name,
        "displayName": display_name,
        "email": email,
        "phone": phone,
        "locale": Value::Null,
        "note": Value::Null,
        "verifiedEmail": true,
        "taxExempt": false,
        "taxExemptions": [],
        "tags": [],
        "state": "ENABLED",
        "dataSaleOptOut": false,
        "canDelete": true,
        "acceptsMarketing": accepts_marketing,
        "metafield": Value::Null,
        "metafields": [],
        "defaultEmailAddress": { "emailAddress": email },
        "defaultPhoneNumber": Value::Null,
        "emailMarketingConsent": {
            "marketingState": if accepts_marketing { "SUBSCRIBED" } else { "NOT_SUBSCRIBED" },
            "marketingOptInLevel": Value::Null,
            "consentUpdatedAt": timestamp
        },
        "smsMarketingConsent": Value::Null,
        "defaultAddress": Value::Null,
        "addressesV2": connection_json_with_empty_edges(Vec::new()),
        "orders": connection_json_with_empty_edges(Vec::new()),
        "numberOfOrders": "0",
        "createdAt": timestamp,
        "updatedAt": timestamp
    })
}

pub(in crate::proxy) fn storefront_customer_json(customer: &Value) -> Value {
    let email = customer.get("email").and_then(Value::as_str);
    let first_name = customer.get("firstName").and_then(Value::as_str);
    let last_name = customer.get("lastName").and_then(Value::as_str);
    let display_name = customer
        .get("displayName")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| storefront_customer_display_name(first_name, last_name, email));
    let accepts_marketing = customer
        .get("acceptsMarketing")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| {
            customer
                .pointer("/emailMarketingConsent/marketingState")
                .and_then(Value::as_str)
                == Some("SUBSCRIBED")
        });
    json!({
        "id": customer.get("id").cloned().unwrap_or(Value::Null),
        "email": customer.get("email").cloned().unwrap_or(Value::Null),
        "firstName": customer.get("firstName").cloned().unwrap_or(Value::Null),
        "lastName": customer.get("lastName").cloned().unwrap_or(Value::Null),
        "displayName": display_name,
        "phone": customer.get("phone").cloned().unwrap_or(Value::Null),
        "acceptsMarketing": accepts_marketing,
        "createdAt": customer.get("createdAt").cloned().unwrap_or(Value::Null),
        "updatedAt": customer.get("updatedAt").cloned().unwrap_or(Value::Null),
        "numberOfOrders": customer.get("numberOfOrders").cloned().unwrap_or_else(|| json!("0")),
        "tags": customer.get("tags").cloned().unwrap_or_else(|| json!([])),
        "defaultAddress": Value::Null,
        "addresses": connection_json_with_empty_edges(Vec::new()),
        "orders": connection_json_with_empty_edges(Vec::new()),
        "avatarUrl": Value::Null,
        "socialLoginProvider": Value::Null,
        "metafield": Value::Null,
        "metafields": []
    })
}

fn storefront_customer_addresses_connection(
    customer: &Value,
    arguments: &BTreeMap<String, ResolvedValue>,
    selection: &[SelectedField],
) -> Value {
    let addresses = customer_address_nodes(customer);
    selected_typed_connection_with_args(
        &addresses,
        arguments,
        selection,
        |address, address_selection| {
            storefront_mailing_address_selected_json(address, address_selection)
        },
        |address| customer_address_cursor(address).unwrap_or_default(),
    )
}

fn storefront_mailing_address_selected_json(address: &Value, selection: &[SelectedField]) -> Value {
    if address.is_null() {
        return Value::Null;
    }
    selected_json(&storefront_mailing_address_json(address), selection)
}

fn storefront_mailing_address_json(address: &Value) -> Value {
    if address.is_null() {
        return Value::Null;
    }
    let mut projected = address.clone();
    if let Some(object) = projected.as_object_mut() {
        if !object.contains_key("countryCode") {
            object.insert(
                "countryCode".to_string(),
                address.get("countryCodeV2").cloned().unwrap_or(Value::Null),
            );
        }
        object
            .entry("formatted".to_string())
            .or_insert_with(|| storefront_formatted_address_lines(address));
        object.entry("latitude".to_string()).or_insert(Value::Null);
        object.entry("longitude".to_string()).or_insert(Value::Null);
    }
    projected
}

fn storefront_formatted_address_lines(address: &Value) -> Value {
    let mut lines = Vec::new();
    for field in ["address1", "address2"] {
        if let Some(value) = address.get(field).and_then(Value::as_str) {
            if !value.is_empty() {
                lines.push(json!(value));
            }
        }
    }
    let locality = ["city", "province", "zip"]
        .into_iter()
        .filter_map(|field| address.get(field).and_then(Value::as_str))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if !locality.is_empty() {
        lines.push(json!(locality));
    }
    if let Some(country) = address.get("country").and_then(Value::as_str) {
        if !country.is_empty() {
            lines.push(json!(country));
        }
    }
    Value::Array(lines)
}

fn storefront_order_json(order: &Value) -> Value {
    let currency_code = order
        .get("currencyCode")
        .or_else(|| order.pointer("/currentTotalPriceSet/shopMoney/currencyCode"))
        .or_else(|| order.pointer("/totalPriceSet/shopMoney/currencyCode"))
        .cloned()
        .unwrap_or_else(|| json!("USD"));
    let total_price = order
        .get("totalPriceV2")
        .or_else(|| order.pointer("/currentTotalPriceSet/shopMoney"))
        .or_else(|| order.pointer("/totalPriceSet/shopMoney"))
        .cloned()
        .unwrap_or_else(|| json!({ "amount": "0.0", "currencyCode": currency_code.clone() }));
    json!({
        "__typename": "Order",
        "id": order.get("id").cloned().unwrap_or(Value::Null),
        "name": order.get("name").cloned().unwrap_or_else(|| json!("")),
        "email": order.get("email").cloned().unwrap_or(Value::Null),
        "phone": order.get("phone").cloned().unwrap_or(Value::Null),
        "currencyCode": currency_code,
        "customerUrl": order.get("customerUrl").cloned().unwrap_or(Value::Null),
        "financialStatus": order.get("displayFinancialStatus").or_else(|| order.get("financialStatus")).cloned().unwrap_or(Value::Null),
        "fulfillmentStatus": order.get("displayFulfillmentStatus").or_else(|| order.get("fulfillmentStatus")).cloned().unwrap_or_else(|| json!("UNFULFILLED")),
        "orderNumber": storefront_order_number(order),
        "processedAt": order.get("processedAt").or_else(|| order.get("createdAt")).cloned().unwrap_or_else(|| json!("1970-01-01T00:00:00Z")),
        "subtotalPriceV2": order.get("subtotalPriceV2").or_else(|| order.pointer("/subtotalPriceSet/shopMoney")).cloned().unwrap_or(Value::Null),
        "totalPrice": total_price.clone(),
        "totalPriceV2": total_price,
        "lineItems": order.get("lineItems").cloned().unwrap_or_else(|| connection_json_with_empty_edges(Vec::new()))
    })
}

fn storefront_order_number(order: &Value) -> Value {
    if let Some(number) = order.get("orderNumber").and_then(Value::as_i64) {
        return json!(number);
    }
    let digits = order
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits
        .parse::<i64>()
        .map(Value::from)
        .unwrap_or_else(|_| json!(0))
}

fn storefront_order_cursor(order: &Value) -> String {
    order
        .get("id")
        .and_then(Value::as_str)
        .map(|id| format!("cursor:{id}"))
        .unwrap_or_default()
}

fn storefront_customer_address_node_index(nodes: &[Value], address_id: &str) -> Option<usize> {
    nodes
        .iter()
        .position(|node| node.get("id").and_then(Value::as_str) == Some(address_id))
}

fn storefront_customer_default_address_id(customer: &Value) -> Option<&str> {
    customer
        .get("defaultAddress")
        .and_then(|address| address.get("id"))
        .and_then(Value::as_str)
}

fn storefront_customer_payload(
    customer: Value,
    customer_access_token: Value,
    customer_user_errors: Vec<Value>,
) -> Value {
    let user_errors = storefront_user_errors_without_code(&customer_user_errors);
    json!({
        "customer": customer,
        "customerAccessToken": customer_access_token,
        "customerUserErrors": customer_user_errors,
        "userErrors": user_errors
    })
}

fn storefront_customer_address_payload_selected(
    address_field: &str,
    address: Value,
    customer_user_errors: Vec<Value>,
    selection: &[SelectedField],
) -> Value {
    let customer_user_errors = storefront_customer_user_errors_with_codes(customer_user_errors);
    selected_payload_json(selection, |field| match field.name.as_str() {
        name if name == address_field => Some(storefront_mailing_address_selected_json(
            &address,
            &field.selection,
        )),
        "customerUserErrors" => Some(selected_user_errors(
            &customer_user_errors,
            &field.selection,
        )),
        "userErrors" => {
            let errors = storefront_user_errors_without_code(&customer_user_errors);
            Some(selected_user_errors(
                errors.as_array().map(Vec::as_slice).unwrap_or(&[]),
                &field.selection,
            ))
        }
        _ => None,
    })
}

fn storefront_customer_address_delete_payload_selected(
    deleted_customer_address_id: Value,
    customer_user_errors: Vec<Value>,
    selection: &[SelectedField],
) -> Value {
    let customer_user_errors = storefront_customer_user_errors_with_codes(customer_user_errors);
    selected_payload_json(selection, |field| match field.name.as_str() {
        "deletedCustomerAddressId" => Some(deleted_customer_address_id.clone()),
        "customerUserErrors" => Some(selected_user_errors(
            &customer_user_errors,
            &field.selection,
        )),
        "userErrors" => {
            let errors = storefront_user_errors_without_code(&customer_user_errors);
            Some(selected_user_errors(
                errors.as_array().map(Vec::as_slice).unwrap_or(&[]),
                &field.selection,
            ))
        }
        _ => None,
    })
}

fn storefront_customer_token_payload(
    customer_access_token: Value,
    customer_user_errors: Vec<Value>,
) -> Value {
    let user_errors = storefront_user_errors_without_code(&customer_user_errors);
    json!({
        "customerAccessToken": customer_access_token,
        "customerUserErrors": customer_user_errors,
        "userErrors": user_errors
    })
}

fn storefront_customer_activation_payload(
    customer: Value,
    customer_access_token: Value,
    customer_user_errors: Vec<Value>,
    include_user_errors: bool,
) -> Value {
    let mut payload = json!({
        "customer": customer,
        "customerAccessToken": customer_access_token,
        "customerUserErrors": customer_user_errors
    });
    if include_user_errors {
        payload["userErrors"] = storefront_plain_user_errors_with_null_field(
            payload["customerUserErrors"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        );
    }
    payload
}

fn storefront_plain_user_errors_with_null_field(errors: &[Value]) -> Value {
    Value::Array(
        errors
            .iter()
            .map(|error| {
                json!({
                    "field": Value::Null,
                    "message": error.get("message").cloned().unwrap_or(Value::Null)
                })
            })
            .collect(),
    )
}

fn storefront_user_errors_without_code(errors: &[Value]) -> Value {
    Value::Array(
        errors
            .iter()
            .map(|error| {
                json!({
                    "field": error.get("field").cloned().unwrap_or(Value::Null),
                    "message": error.get("message").cloned().unwrap_or(Value::Null)
                })
            })
            .collect(),
    )
}

fn storefront_customer_user_errors_with_codes(errors: Vec<Value>) -> Vec<Value> {
    errors
        .into_iter()
        .map(|mut error| {
            if let Some(object) = error.as_object_mut() {
                object.entry("code".to_string()).or_insert(Value::Null);
            }
            error
        })
        .collect()
}

fn storefront_customer_user_error(
    field: impl serde::Serialize,
    message: &str,
    code: Option<&str>,
) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code
    })
}

fn storefront_invalid_customer_access_token_errors() -> Vec<Value> {
    vec![storefront_customer_user_error(
        ["customerAccessToken"],
        "Invalid customer access token",
        Some("INVALID"),
    )]
}

fn storefront_access_denied_error(response_key: &str) -> Value {
    let message = format!(
        "Access denied for {response_key} field. Required access: `unauthenticated_write_customers` access scope. Also: Requires valid customer access token."
    );
    json!({
        "message": message,
        "path": [response_key],
        "locations": [],
        "extensions": {
            "code": "ACCESS_DENIED",
            "documentation": "https://shopify.dev/api/usage/access-scopes",
            "requiredAccess": "`unauthenticated_write_customers` access scope. Also: Requires valid customer access token."
        }
    })
}

fn preserve_storefront_address_phone(
    node: &mut Value,
    address_input: &BTreeMap<String, ResolvedValue>,
) {
    if !address_input.contains_key("phone") {
        return;
    }
    let phone = resolved_string_field(address_input, "phone")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(Value::String)
        .unwrap_or(Value::Null);
    if let Some(object) = node.as_object_mut() {
        object.insert("phone".to_string(), phone);
    }
}

fn storefront_not_found_error(response_key: &str) -> Value {
    json!({
        "message": "Unidentified customer",
        "path": [response_key],
        "locations": [],
        "extensions": { "code": "NOT_FOUND" }
    })
}

fn storefront_customer_display_name(
    first_name: Option<&str>,
    last_name: Option<&str>,
    email: Option<&str>,
) -> String {
    let name = [first_name, last_name]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !name.is_empty() {
        return name;
    }
    email.unwrap_or_default().to_string()
}

fn storefront_customer_state(customer: &Value) -> &str {
    customer
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("DISABLED")
}

fn storefront_customer_password_matches(customer: &Value, password: &str) -> bool {
    let Some(customer_id) = customer.get("id").and_then(Value::as_str) else {
        return false;
    };
    customer
        .get(STOREFRONT_CUSTOMER_PASSWORD_FINGERPRINT_FIELD)
        .and_then(Value::as_str)
        == Some(storefront_password_fingerprint(customer_id, password).as_str())
}

fn storefront_customer_activation_token_for_id(customer_id: &str) -> String {
    let stable_tail = resource_id_tail(customer_id)
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect::<String>();
    if stable_tail.is_empty() {
        "sdp-activation-token".to_string()
    } else {
        format!("sdp-activation-{stable_tail}")
    }
}

fn storefront_access_token_value(customer_id: &str, sequence: u64, expires_at: &str) -> String {
    let seed = format!("{customer_id}:{sequence}:{expires_at}");
    format!(
        "sdp_ca_{}_{}",
        sequence,
        &storefront_sha256_hex(&seed)[..24]
    )
}

fn storefront_password_fingerprint(customer_id: &str, password: &str) -> String {
    storefront_sha256_hex(&format!("storefront-password:{customer_id}:{password}"))
}

fn storefront_token_hash(token: &str) -> String {
    storefront_sha256_hex(&format!("storefront-token:{token}"))
}

pub(in crate::proxy) fn storefront_sha256_hex(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub(in crate::proxy) fn storefront_format_timestamp(timestamp: time::OffsetDateTime) -> String {
    timestamp
        .format(&time::format_description::well_known::Rfc3339)
        .expect("UTC timestamps should format as RFC3339")
}

fn storefront_timestamp_is_future(value: &str, now: time::OffsetDateTime) -> bool {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map(|expires_at| expires_at > now)
        .unwrap_or(false)
}

fn storefront_email_looks_valid(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

pub(in crate::proxy) fn storefront_customer_email_key(email: &str) -> String {
    email.trim().to_ascii_lowercase()
}

fn storefront_customer_contains_html_tag(value: &str) -> bool {
    let Some(start) = value.find('<') else {
        return false;
    };
    value[start..].contains('>')
}

fn storefront_redacted_variables_json(variables: &BTreeMap<String, ResolvedValue>) -> Value {
    let value = resolved_variables_json(variables);
    storefront_redact_sensitive_json(value, None)
}

pub(in crate::proxy) fn storefront_redact_sensitive_json(value: Value, key: Option<&str>) -> Value {
    if key.is_some_and(storefront_sensitive_customer_auth_key) {
        return json!("<redacted:storefront-customer-auth>");
    }
    match value {
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| storefront_redact_sensitive_json(value, None))
                .collect(),
        ),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    let redacted = storefront_redact_sensitive_json(value, Some(&key));
                    (key, redacted)
                })
                .collect(),
        ),
        other => other,
    }
}

fn storefront_sensitive_customer_auth_key(key: &str) -> bool {
    matches!(
        key,
        "password"
            | "customerAccessToken"
            | "accessToken"
            | "activationToken"
            | "activationUrl"
            | "resetToken"
            | "resetUrl"
            | "token"
            | "multipassToken"
    )
}
