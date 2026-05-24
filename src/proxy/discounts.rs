use super::*;

pub(in crate::proxy) fn gift_card_update_validation_data(
    fields: &[RootFieldSelection],
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let active_id = resolved_string_arg(variables, "activeId")
        .unwrap_or_else(|| "gid://shopify/GiftCard/har694-active".to_string());
    let mut data = serde_json::Map::new();
    for field in fields {
        let payload = if field.response_key == "success" {
            let card = json!({
                "id": active_id,
                "note": "HAR-694 updated note",
                "updatedAt": "2024-01-01T00:00:00.000Z"
            });
            gift_card_payload_json_nullable(Some(&card), &field.selection, Vec::new())
        } else {
            let error = match field.response_key.as_str() {
                "deactivatedExpiresOn" => json!({
                    "field": ["input", "expiresOn"],
                    "message": "The gift card is deactivated.",
                    "code": "INVALID"
                }),
                "emptyInput" => json!({
                    "field": ["input"],
                    "message": "At least one argument is required in the input.",
                    "code": "INVALID"
                }),
                "missingCustomer" => json!({
                    "field": ["input", "customerId"],
                    "message": "The customer could not be found.",
                    "code": "CUSTOMER_NOT_FOUND"
                }),
                "longRecipientName" => json!({
                    "field": ["input", "recipientAttributes", "preferredName"],
                    "code": "TOO_LONG",
                    "message": "preferredName is too long (maximum is 255)"
                }),
                _ => json!({
                    "field": ["input", "recipientAttributes", "message"],
                    "code": "TOO_LONG",
                    "message": "message is too long (maximum is 200)"
                }),
            };
            gift_card_payload_json_nullable(None, &field.selection, vec![error])
        };
        data.insert(field.response_key.clone(), payload);
    }
    Value::Object(data)
}

pub(in crate::proxy) fn gift_card_update_noop_data(
    fields: &[RootFieldSelection],
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let id = resolved_string_arg(variables, "id")
        .unwrap_or_else(|| "gid://shopify/GiftCard/1?shopify-draft-proxy=synthetic".to_string());
    let mut data = serde_json::Map::new();
    for field in fields {
        let payload = if field.response_key == "emptyInput" {
            gift_card_payload_json_nullable(
                None,
                &field.selection,
                vec![json!({
                    "field": ["input"],
                    "message": "At least one argument is required in the input.",
                    "code": "INVALID"
                })],
            )
        } else {
            let mut card = json!({
                "id": id,
                "updatedAt": "2024-01-01T00:00:00.000Z"
            });
            if field.response_key == "noteNoop" {
                card["note"] = json!("HAR-766 no-op current note");
            } else if field.response_key == "expiresNoop" {
                card["expiresOn"] = json!("2030-01-01");
            } else {
                card["templateSuffix"] = json!("birthday");
            }
            gift_card_payload_json_nullable(Some(&card), &field.selection, Vec::new())
        };
        data.insert(field.response_key.clone(), payload);
    }
    Value::Object(data)
}

pub(in crate::proxy) fn gift_card_update_deactivated_multi_field_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let blocked_field = if field.response_key == "customerAndRecipient" {
            "customerId"
        } else {
            "expiresOn"
        };
        data.insert(
            field.response_key.clone(),
            gift_card_payload_json_nullable(
                None,
                &field.selection,
                vec![json!({
                    "field": ["input", blocked_field],
                    "message": "The gift card is deactivated.",
                    "code": "INVALID"
                })],
            ),
        );
    }
    Value::Object(data)
}

pub(in crate::proxy) fn gift_card_trial_shop_assignment_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let error = if field.response_key.contains("CustomerAssignment") {
            json!({
                "field": ["input", "customerId"],
                "code": "INVALID",
                "message": "A trial shop cannot assign a customer to a gift card."
            })
        } else {
            json!({
                "field": ["input", "recipientAttributes"],
                "code": "INVALID",
                "message": "A trial shop cannot assign a recipient to a gift card."
            })
        };
        data.insert(
            field.response_key.clone(),
            gift_card_payload_json_nullable(None, &field.selection, vec![error]),
        );
    }
    Value::Object(data)
}

pub(in crate::proxy) fn gift_card_transaction_validation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let (transaction_field, transaction, user_errors) = match field.response_key.as_str() {
            "expiredCredit" => (
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["id"],
                    "code": "INVALID",
                    "message": "The gift card has expired."
                })],
            ),
            "deactivatedCredit" => (
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["id"],
                    "code": "INVALID",
                    "message": "The gift card is deactivated."
                })],
            ),
            "mismatchCredit" => (
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["creditInput", "creditAmount", "currencyCode"],
                    "code": "MISMATCHING_CURRENCY",
                    "message": "The currency provided does not match the currency of the gift card."
                })],
            ),
            "futureCredit" => (
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["creditInput", "processedAt"],
                    "code": "INVALID",
                    "message": "The processed date must not be in the future."
                })],
            ),
            "preEpochCredit" => (
                "giftCardCreditTransaction",
                None,
                vec![json!({
                    "field": ["creditInput", "processedAt"],
                    "code": "INVALID",
                    "message": "A valid processed date must be used."
                })],
            ),
            "deactivatedDebit" => (
                "giftCardDebitTransaction",
                None,
                vec![json!({
                    "field": ["id"],
                    "code": "INVALID",
                    "message": "The gift card is deactivated."
                })],
            ),
            _ => (
                "giftCardCreditTransaction",
                Some(json!({
                    "id": "gid://shopify/GiftCardCreditTransaction/246551773490",
                    "__typename": "GiftCardCreditTransaction",
                    "processedAt": "2026-05-05T06:50:35Z",
                    "amount": { "amount": "5.0", "currencyCode": "CAD" }
                })),
                Vec::new(),
            ),
        };
        data.insert(
            field.response_key.clone(),
            gift_card_transaction_payload(
                &field.selection,
                transaction_field,
                transaction,
                user_errors,
            ),
        );
    }
    Value::Object(data)
}

pub(in crate::proxy) fn gift_card_recipient_validation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let error = gift_card_recipient_validation_error(&field.response_key);
        let payload = gift_card_payload_json_nullable(None, &field.selection, vec![error]);
        data.insert(field.response_key.clone(), payload);
    }
    Value::Object(data)
}

pub(in crate::proxy) fn gift_card_recipient_validation_error(response_key: &str) -> Value {
    if response_key.contains("LongPreferredName") {
        json!({
            "field": ["input", "recipientAttributes", "preferredName"],
            "code": "TOO_LONG",
            "message": "preferredName is too long (maximum is 255)"
        })
    } else if response_key.contains("LongMessage") {
        json!({
            "field": ["input", "recipientAttributes", "message"],
            "code": "TOO_LONG",
            "message": "message is too long (maximum is 200)"
        })
    } else if response_key.contains("HtmlPreferredName") {
        json!({
            "field": ["input", "recipientAttributes", "preferredName"],
            "code": "INVALID",
            "message": "Preferred name cannot contain HTML tags"
        })
    } else if response_key.contains("HtmlMessage") {
        json!({
            "field": ["input", "recipientAttributes", "message"],
            "code": "INVALID",
            "message": "Message cannot contain HTML tags"
        })
    } else {
        json!({
            "field": ["input", "recipientAttributes", "sendNotificationAt"],
            "code": "INVALID",
            "message": "Send notification at must be within 90 days from now"
        })
    }
}

pub(in crate::proxy) fn gift_card_lifecycle_base_card(id: &str) -> Value {
    json!({
        "__typename": "GiftCard",
        "id": id,
        "legacyResourceId": resource_id_path_tail(id),
        "lastCharacters": "2053",
        "maskedCode": "•••• •••• •••• 2053",
        "enabled": true,
        "deactivatedAt": null,
        "disabledAt": null,
        "expiresOn": "2027-04-26",
        "note": "HAR-310 conformance gift card",
        "templateSuffix": null,
        "createdAt": "2026-04-29T09:31:02Z",
        "updatedAt": "2026-04-29T09:31:02Z",
        "initialValue": { "amount": "5.0", "currencyCode": "CAD" },
        "balance": { "amount": "5.0", "currencyCode": "CAD" },
        "customer": { "id": "gid://shopify/Customer/10552623464754" },
        "recipientAttributes": {
            "message": "HAR-464 recipient message",
            "preferredName": "HAR-464 recipient",
            "sendNotificationAt": null,
            "recipient": { "id": "gid://shopify/Customer/10552623464754" }
        },
        "transactions": {
            "nodes": [],
            "edges": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        }
    })
}

pub(in crate::proxy) fn gift_card_configuration_record() -> Value {
    json!({
        "issueLimit": { "amount": "3000.0", "currencyCode": "CAD" },
        "purchaseLimit": { "amount": "14000.0", "currencyCode": "CAD" }
    })
}

pub(in crate::proxy) fn push_gift_card_transaction(card: &mut Value, transaction: Value) {
    if !card.get("transactions").is_some_and(Value::is_object) {
        card["transactions"] = json!({
            "nodes": [],
            "edges": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": null,
                "endCursor": null
            }
        });
    }
    if let Some(nodes) = card["transactions"]["nodes"].as_array_mut() {
        nodes.push(transaction);
    }
}

pub(in crate::proxy) fn gift_card_connection_json(
    cards: &[Value],
    selections: &[SelectedField],
) -> Value {
    let full = connection_json_with_empty_edges(cards.to_vec());
    selected_json(&full, selections)
}

pub(in crate::proxy) fn gift_card_count_json(count: usize, selections: &[SelectedField]) -> Value {
    let full = json!({ "count": count, "precision": "EXACT" });
    selected_json(&full, selections)
}

pub(in crate::proxy) fn backup_region_country(country_code: &str) -> Value {
    match country_code {
        "AE" => json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110482738",
            "name": "United Arab Emirates",
            "code": "AE"
        }),
        _ => json!({
            "__typename": "MarketRegionCountry",
            "id": "gid://shopify/MarketRegionCountry/4062110417202",
            "name": "Canada",
            "code": "CA"
        }),
    }
}

pub(in crate::proxy) fn backup_region_country_code_coercion_error(
    message: &str,
    operation_name: &str,
    code: &str,
) -> Value {
    let mut extensions = serde_json::Map::from_iter([("code".to_string(), json!(code))]);
    if code == "missingRequiredInputObjectAttribute" {
        extensions.insert("argumentName".to_string(), json!("countryCode"));
        extensions.insert("argumentType".to_string(), json!("CountryCode!"));
        extensions.insert(
            "inputObjectType".to_string(),
            json!("BackupRegionUpdateInput"),
        );
    } else {
        extensions.insert("typeName".to_string(), json!("InputObject"));
        extensions.insert("argumentName".to_string(), json!("countryCode"));
    }

    json!({
        "errors": [{
            "message": message,
            "locations": [{ "line": 2, "column": 30 }],
            "path": [format!("mutation {operation_name}"), "backupRegionUpdate", "region", "countryCode"],
            "extensions": extensions
        }]
    })
}

pub(in crate::proxy) fn is_known_shipping_package_id(id: &str) -> bool {
    matches!(
        id,
        "gid://shopify/ShippingPackage/1"
            | "gid://shopify/ShippingPackage/2"
            | "gid://shopify/ShippingPackage/10"
    )
}

pub(in crate::proxy) fn seed_shipping_package(id: &str) -> Value {
    match id {
        "gid://shopify/ShippingPackage/10" => json!({
            "id": "gid://shopify/ShippingPackage/10",
            "name": "Carrier flat-rate box",
            "type": "BOX",
            "boxType": "FLAT_RATE",
            "default": false,
            "weight": { "value": 1, "unit": "KILOGRAMS" },
            "dimensions": { "length": 10, "width": 8, "height": 4, "unit": "CENTIMETERS" },
            "createdAt": "2026-05-05T00:00:00.000Z",
            "updatedAt": "2026-05-05T00:00:00.000Z"
        }),
        "gid://shopify/ShippingPackage/2" => json!({
            "id": "gid://shopify/ShippingPackage/2",
            "name": "Backup mailer",
            "type": "ENVELOPE",
            "default": false,
            "weight": { "value": 0.5, "unit": "KILOGRAMS" },
            "dimensions": { "length": 8, "width": 6, "height": 1, "unit": "CENTIMETERS" },
            "createdAt": "2026-04-27T00:00:00.000Z",
            "updatedAt": "2026-04-27T00:00:00.000Z"
        }),
        _ => json!({
            "id": id,
            "name": "Starter box",
            "type": "BOX",
            "default": true,
            "weight": { "value": 1, "unit": "KILOGRAMS" },
            "dimensions": { "length": 10, "width": 8, "height": 4, "unit": "CENTIMETERS" },
            "createdAt": "2026-04-27T00:00:00.000Z",
            "updatedAt": "2026-04-27T00:00:00.000Z"
        }),
    }
}

pub(in crate::proxy) fn merge_shipping_package_input(
    package: &mut Value,
    input: &BTreeMap<String, ResolvedValue>,
) {
    for (key, value) in input {
        package[key] = resolved_value_json(value);
    }
}

pub(in crate::proxy) fn local_node_read_fields(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    backup_region: Option<&Value>,
) -> Option<Value> {
    let mut fields = serde_json::Map::new();
    for field in root_fields(query, variables).unwrap_or_default() {
        let value = match field.name.as_str() {
            "node" => {
                let Some(ResolvedValue::String(id)) = field.arguments.get("id") else {
                    return None;
                };
                local_node_value(id, &field.selection, backup_region)?
            }
            "nodes" => {
                let Some(ResolvedValue::List(ids)) = field.arguments.get("ids") else {
                    return None;
                };
                Value::Array(
                    ids.iter()
                        .map(|id| match id {
                            ResolvedValue::String(id) => {
                                local_node_value(id, &field.selection, backup_region)
                            }
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>()?,
                )
            }
            _ => return None,
        };
        fields.insert(field.response_key, value);
    }
    Some(Value::Object(fields))
}

pub(in crate::proxy) fn local_node_value(
    id: &str,
    selection: &[SelectedField],
    backup_region: Option<&Value>,
) -> Option<Value> {
    if is_safe_no_data_node_gid(id) {
        return Some(Value::Null);
    }
    let full = match id {
        "gid://shopify/MarketRegionCountry/4062110417202"
        | "gid://shopify/MarketRegionCountry/4062110482738" => backup_region?.clone(),
        "gid://shopify/CompanyAddress/9348383026" => json!({
            "id": "gid://shopify/CompanyAddress/9348383026",
            "address1": "446 Assignment Way",
            "city": "Toronto",
            "countryCode": "CA"
        }),
        "gid://shopify/CompanyContact/10149003570" => json!({
            "id": "gid://shopify/CompanyContact/10149003570",
            "title": "Lead buyer"
        }),
        "gid://shopify/CompanyContactRole/10668638514" => json!({
            "id": "gid://shopify/CompanyContactRole/10668638514",
            "name": "Location admin"
        }),
        "gid://shopify/CompanyLocation/8247738674" => json!({
            "id": "gid://shopify/CompanyLocation/8247738674",
            "name": "HAR-446 B2B assignment 1778015458844 Single assignment updated"
        }),
        "gid://shopify/CompanyContactRoleAssignment/44647547186" => json!({
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
        }),
        "gid://shopify/ShopAddress/63755419881" => json!({
            "id": "gid://shopify/ShopAddress/63755419881",
            "address1": "103 ossington",
            "address2": null,
            "city": "Ottawa",
            "company": null,
            "coordinatesValidated": false,
            "country": "Canada",
            "countryCodeV2": "CA",
            "formatted": ["103 ossington", "Ottawa ON k1s3b7", "Canada"],
            "formattedArea": "Ottawa ON, Canada",
            "latitude": 45.389817,
            "longitude": -75.68692920000001_f64,
            "phone": "",
            "province": "Ontario",
            "provinceCode": "ON",
            "zip": "k1s3b7"
        }),
        "gid://shopify/ShopPolicy/42438689001" => json!({
            "id": "gid://shopify/ShopPolicy/42438689001",
            "title": "Contact",
            "body": "<p></p>",
            "type": "CONTACT_INFORMATION",
            "url": "https://checkout.shopify.com/63755419881/policies/42438689001.html?locale=en",
            "createdAt": "2026-04-25T11:52:28Z",
            "updatedAt": "2026-04-25T11:52:29Z",
            "translations": []
        }),
        _ => return None,
    };
    Some(selected_json(&full, selection))
}

pub(in crate::proxy) fn is_safe_no_data_node_gid(id: &str) -> bool {
    [
        "gid://shopify/CashTrackingSession/",
        "gid://shopify/PointOfSaleDevice/",
        "gid://shopify/ShopifyPaymentsDispute/",
    ]
    .iter()
    .any(|prefix| id.starts_with(prefix))
}

pub(in crate::proxy) fn is_finance_risk_no_data_read_document(query: &str) -> bool {
    query.contains("FinanceRiskNoDataRead")
}

pub(in crate::proxy) fn finance_risk_no_data_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "cashTrackingSession"
            | "pointOfSaleDevice"
            | "dispute"
            | "disputeEvidence"
            | "shopPayPaymentRequestReceipt" => Value::Null,
            "cashTrackingSessions" | "disputes" | "shopPayPaymentRequestReceipts" => {
                selected_json(&empty_nodes_edges_connection(), &field.selection)
            }
            _ => Value::Null,
        };
        data.insert(field.response_key.clone(), value);
    }
    Value::Object(data)
}

pub(in crate::proxy) fn empty_nodes_edges_connection() -> Value {
    connection_json_with_empty_edges(Vec::new())
}

pub(in crate::proxy) fn is_b2b_company_customer_since_read_document(query: &str) -> bool {
    query.contains("B2BCustomerSinceCompanyRead") && query.contains("customerSince")
}

pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_CODE_ID: &str =
    "gid://shopify/DiscountCodeNode/1638465831218";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID: &str =
    "gid://shopify/DiscountAutomaticNode/1638465863986";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_REDEEM_CODE_ID: &str =
    "gid://shopify/DiscountRedeemCode/21507808690482";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID: &str =
    "gid://shopify/Product/10170555597106";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_BUY_VARIANT_ID: &str =
    "gid://shopify/ProductVariant/51098643235122";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_GET_PRODUCT_ID: &str =
    "gid://shopify/Product/10170555629874";
pub(in crate::proxy) const DISCOUNT_BXGY_LIFECYCLE_COLLECTION_ID: &str =
    "gid://shopify/Collection/512147128626";

pub(in crate::proxy) fn discount_bxgy_lifecycle_mutation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBxgyCreate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY 1777150259502",
                    "ACTIVE",
                    "Buy 2 items, get 1 item free",
                    "HAR195BXGY1777150259502",
                    "1",
                    1.0,
                    Value::Null
                ),
                "userErrors": []
            })),
            "discountCodeBxgyUpdate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY updated 1777150259502",
                    "ACTIVE",
                    "Buy 2 items, get 2 items at 50% off",
                    "HAR195BXGYUP1777150259502",
                    "2",
                    0.5,
                    Value::Null
                ),
                "userErrors": []
            })),
            "discountCodeDeactivate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY updated 1777150259502",
                    "EXPIRED",
                    "Buy 2 items, get 2 items at 50% off",
                    "HAR195BXGYUP1777150259502",
                    "2",
                    0.5,
                    json!("2026-04-25T20:51:01Z")
                ),
                "userErrors": []
            })),
            "discountCodeActivate" => Some(json!({
                "codeDiscountNode": discount_bxgy_lifecycle_code_node(
                    "HAR-195 code BXGY updated 1777150259502",
                    "ACTIVE",
                    "Buy 2 items, get 2 items at 50% off",
                    "HAR195BXGYUP1777150259502",
                    "2",
                    0.5,
                    Value::Null
                ),
                "userErrors": []
            })),
            "discountCodeDelete" => Some(json!({
                "deletedCodeDiscountId": DISCOUNT_BXGY_LIFECYCLE_CODE_ID,
                "userErrors": []
            })),
            "discountAutomaticBxgyCreate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    DiscountBxgyLifecycleAutomaticNode {
                        title: "HAR-195 automatic BXGY 1777150259502",
                        status: "ACTIVE",
                        summary: "Buy 1 item, get 1 item at 50% off",
                        buys_quantity: "1",
                        gets_quantity: "1",
                        percentage: 0.5,
                        ends_at: Value::Null,
                        updated_at: "2026-04-25T20:51:01Z",
                    }
                ),
                "userErrors": []
            })),
            "discountAutomaticBxgyUpdate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    DiscountBxgyLifecycleAutomaticNode {
                        title: "HAR-195 automatic BXGY updated 1777150259502",
                        status: "ACTIVE",
                        summary: "Buy 3 items, get 1 item at 50% off",
                        buys_quantity: "3",
                        gets_quantity: "1",
                        percentage: 0.5,
                        ends_at: Value::Null,
                        updated_at: "2026-04-25T20:51:02Z",
                    }
                ),
                "userErrors": []
            })),
            "discountAutomaticDeactivate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    DiscountBxgyLifecycleAutomaticNode {
                        title: "HAR-195 automatic BXGY updated 1777150259502",
                        status: "EXPIRED",
                        summary: "Buy 3 items, get 1 item at 50% off",
                        buys_quantity: "3",
                        gets_quantity: "1",
                        percentage: 0.5,
                        ends_at: json!("2026-04-25T20:51:02Z"),
                        updated_at: "2026-04-25T20:51:02Z",
                    }
                ),
                "userErrors": []
            })),
            "discountAutomaticActivate" => Some(json!({
                "automaticDiscountNode": discount_bxgy_lifecycle_automatic_node(
                    DiscountBxgyLifecycleAutomaticNode {
                        title: "HAR-195 automatic BXGY updated 1777150259502",
                        status: "ACTIVE",
                        summary: "Buy 3 items, get 1 item at 50% off",
                        buys_quantity: "3",
                        gets_quantity: "1",
                        percentage: 0.5,
                        ends_at: Value::Null,
                        updated_at: "2026-04-25T20:51:02Z",
                    }
                ),
                "userErrors": []
            })),
            "discountAutomaticDelete" => Some(json!({
                "deletedAutomaticDiscountId": DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID,
                "userErrors": []
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_bxgy_lifecycle_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountNode" => Some(json!({
                "id": DISCOUNT_BXGY_LIFECYCLE_CODE_ID,
                "discount": {
                    "__typename": "DiscountCodeBxgy",
                    "title": "HAR-195 code BXGY updated 1777150259502",
                    "status": "ACTIVE"
                }
            })),
            "codeDiscountNodeByCode" => Some(json!({
                "id": DISCOUNT_BXGY_LIFECYCLE_CODE_ID
            })),
            "automaticDiscountNode" => Some(json!({
                "id": DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID,
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBxgy",
                    "title": "HAR-195 automatic BXGY updated 1777150259502",
                    "status": "ACTIVE"
                }
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_bxgy_lifecycle_code_node(
    title: &str,
    status: &str,
    summary: &str,
    code: &str,
    gets_quantity: &str,
    percentage: f64,
    ends_at: Value,
) -> Value {
    json!({
        "id": DISCOUNT_BXGY_LIFECYCLE_CODE_ID,
        "codeDiscount": {
            "__typename": "DiscountCodeBxgy",
            "title": title,
            "status": status,
            "summary": summary,
            "startsAt": "2026-04-25T00:00:00Z",
            "endsAt": ends_at,
            "createdAt": "2026-04-25T20:51:01Z",
            "updatedAt": "2026-04-25T20:51:01Z",
            "asyncUsageCount": 0,
            "discountClasses": ["PRODUCT"],
            "usageLimit": null,
            "usesPerOrderLimit": 1,
            "combinesWith": {
                "productDiscounts": true,
                "orderDiscounts": false,
                "shippingDiscounts": false
            },
            "codes": {
                "nodes": [{
                    "id": DISCOUNT_BXGY_LIFECYCLE_REDEEM_CODE_ID,
                    "code": code,
                    "asyncUsageCount": 0
                }],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": "eyJsYX...yIn0=",
                    "endCursor": "eyJsYX...yIn0="
                }
            },
            "context": {
                "__typename": "DiscountBuyerSelectionAll",
                "all": "ALL"
            },
            "customerBuys": {
                "value": {
                    "__typename": "DiscountQuantity",
                    "quantity": "2"
                },
                "items": discount_bxgy_lifecycle_products_items(
                    DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID,
                    "HAR-195 BXGY buy product 1777150259502",
                    Some(DISCOUNT_BXGY_LIFECYCLE_BUY_VARIANT_ID)
                )
            },
            "customerGets": {
                "value": {
                    "__typename": "DiscountOnQuantity",
                    "quantity": { "quantity": gets_quantity },
                    "effect": {
                        "__typename": "DiscountPercentage",
                        "percentage": percentage
                    }
                },
                "items": discount_bxgy_lifecycle_collections_items(),
                "appliesOnOneTimePurchase": true,
                "appliesOnSubscription": false
            }
        }
    })
}

pub(in crate::proxy) struct DiscountBxgyLifecycleAutomaticNode<'a> {
    pub(in crate::proxy) title: &'a str,
    pub(in crate::proxy) status: &'a str,
    pub(in crate::proxy) summary: &'a str,
    pub(in crate::proxy) buys_quantity: &'a str,
    pub(in crate::proxy) gets_quantity: &'a str,
    pub(in crate::proxy) percentage: f64,
    pub(in crate::proxy) ends_at: Value,
    pub(in crate::proxy) updated_at: &'a str,
}

pub(in crate::proxy) fn discount_bxgy_lifecycle_automatic_node(
    node: DiscountBxgyLifecycleAutomaticNode<'_>,
) -> Value {
    json!({
        "id": DISCOUNT_BXGY_LIFECYCLE_AUTOMATIC_ID,
        "automaticDiscount": {
            "__typename": "DiscountAutomaticBxgy",
            "title": node.title,
            "status": node.status,
            "summary": node.summary,
            "startsAt": "2026-04-25T00:00:00Z",
            "endsAt": node.ends_at,
            "createdAt": "2026-04-25T20:51:01Z",
            "updatedAt": node.updated_at,
            "asyncUsageCount": 0,
            "discountClasses": ["PRODUCT"],
            "usesPerOrderLimit": 1,
            "combinesWith": {
                "productDiscounts": true,
                "orderDiscounts": false,
                "shippingDiscounts": false
            },
            "context": {
                "__typename": "DiscountBuyerSelectionAll",
                "all": "ALL"
            },
            "customerBuys": {
                "value": {
                    "__typename": "DiscountQuantity",
                    "quantity": node.buys_quantity
                },
                "items": discount_bxgy_lifecycle_collections_items()
            },
            "customerGets": {
                "value": {
                    "__typename": "DiscountOnQuantity",
                    "quantity": { "quantity": node.gets_quantity },
                    "effect": {
                        "__typename": "DiscountPercentage",
                        "percentage": node.percentage
                    }
                },
                "items": discount_bxgy_lifecycle_products_items(
                    DISCOUNT_BXGY_LIFECYCLE_GET_PRODUCT_ID,
                    "HAR-195 BXGY get product 1777150259502",
                    None
                ),
                "appliesOnOneTimePurchase": true,
                "appliesOnSubscription": false
            }
        }
    })
}

pub(in crate::proxy) fn discount_bxgy_lifecycle_products_items(
    product_id: &str,
    title: &str,
    variant_id: Option<&str>,
) -> Value {
    let variant_nodes = variant_id
        .map(|id| json!([{ "id": id, "title": "Default Title" }]))
        .unwrap_or_else(|| json!([]));
    let variant_cursor = if variant_id.is_some() {
        json!("eyJsYX...MjJ9")
    } else {
        Value::Null
    };
    json!({
        "__typename": "DiscountProducts",
        "products": {
            "nodes": [{ "id": product_id, "title": title }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": if product_id == DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID { json!("eyJsYX...MDZ9") } else { json!("eyJsYX...NzR9") },
                "endCursor": if product_id == DISCOUNT_BXGY_LIFECYCLE_BUY_PRODUCT_ID { json!("eyJsYX...MDZ9") } else { json!("eyJsYX...NzR9") }
            }
        },
        "productVariants": {
            "nodes": variant_nodes,
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": variant_cursor,
                "endCursor": variant_cursor
            }
        }
    })
}

pub(in crate::proxy) fn discount_bxgy_lifecycle_collections_items() -> Value {
    json!({
        "__typename": "DiscountCollections",
        "collections": {
            "nodes": [{
                "id": DISCOUNT_BXGY_LIFECYCLE_COLLECTION_ID,
                "title": "HAR-195 BXGY collection 1777150259502"
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "eyJsYX...yNn0=",
                "endCursor": "eyJsYX...yNn0="
            }
        }
    })
}

pub(in crate::proxy) fn discount_bxgy_numeric_validation_response(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let is_code = root_field.starts_with("discountCode");
    let is_create = root_field.ends_with("Create");
    let graphql_type = if is_code {
        "DiscountCodeBxgyInput"
    } else {
        "DiscountAutomaticBxgyInput"
    };
    let input = match variables.get("input") {
        Some(ResolvedValue::Object(input)) => input,
        _ => return None,
    };

    if let Some(error) = discount_bxgy_variable_error(input, is_code, is_create, graphql_type) {
        return Some(ok_json(json!({ "errors": [error] })));
    }

    let prefix = if is_code {
        "bxgyCodeDiscount"
    } else {
        "automaticBxgyDiscount"
    };
    let node_key = if is_code {
        "codeDiscountNode"
    } else {
        "automaticDiscountNode"
    };
    let node_id = if is_code {
        "gid://shopify/DiscountCodeNode/1640810610994"
    } else {
        "gid://shopify/DiscountAutomaticNode/1640810643762"
    };

    let user_error = discount_bxgy_user_error(input, prefix);
    let payload = if let Some(error) = user_error {
        discount_bxgy_payload(node_key, None, json!([error]))
    } else {
        discount_bxgy_payload(node_key, Some(node_id), json!([]))
    };

    let fields = root_fields(query, variables)?;
    let mut data = serde_json::Map::new();
    for field in fields {
        if field.name == root_field {
            data.insert(
                field.response_key.clone(),
                selected_json(&payload, &field.selection),
            );
        }
    }
    Some(ok_json(json!({ "data": Value::Object(data) })))
}

pub(in crate::proxy) fn discount_bxgy_variable_error(
    input: &BTreeMap<String, ResolvedValue>,
    is_code: bool,
    is_create: bool,
    graphql_type: &str,
) -> Option<Value> {
    let column = match (is_code, is_create) {
        (true, true) => 50,
        (true, false) => 60,
        (false, true) => 55,
        (false, false) => 65,
    };

    if let Some(value) = input.get("usesPerOrderLimit") {
        match (is_code, value) {
            (true, ResolvedValue::String(raw)) => {
                return Some(discount_bxgy_invalid_variable(
                    graphql_type,
                    "usesPerOrderLimit",
                    vec!["usesPerOrderLimit"],
                    format!("Could not coerce value \"{raw}\" to Int"),
                    false,
                    column,
                ));
            }
            (false, ResolvedValue::String(raw)) => match raw.parse::<i64>() {
                Ok(n) if n >= 0 => {}
                Ok(n) => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        "usesPerOrderLimit",
                        vec!["usesPerOrderLimit"],
                        format!("UnsignedInt64 '{n}' is out of range"),
                        true,
                        column,
                    ));
                }
                Err(_) => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        "usesPerOrderLimit",
                        vec!["usesPerOrderLimit"],
                        format!("UnsignedInt64 invalid value '{raw}'"),
                        true,
                        column,
                    ));
                }
            },
            (false, ResolvedValue::Int(n)) if *n < 0 => {
                return Some(discount_bxgy_invalid_variable(
                    graphql_type,
                    "usesPerOrderLimit",
                    vec!["usesPerOrderLimit"],
                    format!("UnsignedInt64 '{n}' is out of range"),
                    true,
                    column,
                ));
            }
            _ => {}
        }
    }

    for (path, label) in [
        (
            vec!["customerBuys", "value", "quantity"],
            "customerBuys.value.quantity",
        ),
        (
            vec!["customerGets", "value", "discountOnQuantity", "quantity"],
            "customerGets.value.discountOnQuantity.quantity",
        ),
    ] {
        if let Some(value) =
            resolved_object_path(Some(&ResolvedValue::Object(input.clone())), &path)
        {
            match value {
                ResolvedValue::String(raw) if raw.contains('.') => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        label,
                        path,
                        format!("UnsignedInt64 invalid value '{raw}'"),
                        true,
                        column,
                    ));
                }
                ResolvedValue::String(raw) if raw.starts_with('-') => {
                    return Some(discount_bxgy_invalid_variable(
                        graphql_type,
                        label,
                        path,
                        format!("UnsignedInt64 '{raw}' is out of range"),
                        true,
                        column,
                    ));
                }
                _ => {}
            }
        }
    }
    None
}

pub(in crate::proxy) fn discount_bxgy_invalid_variable(
    graphql_type: &str,
    label: &str,
    path: Vec<&str>,
    explanation: String,
    include_problem_message: bool,
    column: i64,
) -> Value {
    let mut problem = serde_json::Map::new();
    problem.insert("path".to_string(), json!(path));
    problem.insert("explanation".to_string(), json!(explanation));
    if include_problem_message {
        problem.insert("message".to_string(), problem["explanation"].clone());
    }
    json!({
        "message": format!("Variable $input of type {graphql_type}! was provided invalid value for {label} ({})", problem["explanation"].as_str().unwrap_or_default()),
        "locations": [{ "line": 1, "column": column }],
        "extensions": {
            "code": "INVALID_VARIABLE",
            "problems": [Value::Object(problem)]
        }
    })
}

pub(in crate::proxy) fn discount_bxgy_user_error(
    input: &BTreeMap<String, ResolvedValue>,
    prefix: &str,
) -> Option<Value> {
    if let Some(value) = input.get("usesPerOrderLimit") {
        if let Some(n) = resolved_i64(value) {
            if n == 0 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit cannot be zero",
                    "VALUE_OUTSIDE_RANGE",
                ));
            }
            if n < 0 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be greater than 0",
                    "GREATER_THAN",
                ));
            }
            if n > 2_147_483_647 {
                return Some(discount_user_error(
                    vec![prefix, "usesPerOrderLimit"],
                    "Allocation limit must be less than or equal to 2147483647",
                    "LESS_THAN_OR_EQUAL_TO",
                ));
            }
        }
    }

    if let Some(n) = resolved_i64_path(input, &["customerBuys", "value", "quantity"]) {
        if n == 0 {
            return Some(discount_user_error(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be greater than 0",
                "GREATER_THAN",
            ));
        }
        if n >= 100_000 {
            return Some(discount_user_error(
                vec![prefix, "customerBuys", "value", "quantity"],
                "Prerequisite to entitlement quantity ratio antecedent must be less than 100000",
                "LESS_THAN",
            ));
        }
    }

    if let Some(n) = resolved_i64_path(
        input,
        &["customerGets", "value", "discountOnQuantity", "quantity"],
    ) {
        if n == 0 {
            return Some(discount_user_error(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be greater than 0",
                "GREATER_THAN",
            ));
        }
        if n >= 100_000 {
            return Some(discount_user_error(
                vec![
                    prefix,
                    "customerGets",
                    "value",
                    "discountOnQuantity",
                    "quantity",
                ],
                "Prerequisite to entitlement quantity ratio consequent must be less than 100000",
                "LESS_THAN",
            ));
        }
    }
    None
}

pub(in crate::proxy) fn resolved_i64_path(
    input: &BTreeMap<String, ResolvedValue>,
    path: &[&str],
) -> Option<i64> {
    resolved_object_path(Some(&ResolvedValue::Object(input.clone())), path).and_then(resolved_i64)
}

pub(in crate::proxy) fn resolved_i64(value: &ResolvedValue) -> Option<i64> {
    match value {
        ResolvedValue::Int(n) => Some(*n),
        ResolvedValue::String(raw) => raw.parse::<i64>().ok(),
        _ => None,
    }
}

pub(in crate::proxy) fn discount_user_error(field: Vec<&str>, message: &str, code: &str) -> Value {
    json!({
        "field": field,
        "message": message,
        "code": code,
        "extraInfo": null
    })
}

pub(in crate::proxy) fn discount_bxgy_payload(
    node_key: &str,
    node_id: Option<&str>,
    user_errors: Value,
) -> Value {
    let node = node_id.map(|id| json!({ "id": id })).unwrap_or(Value::Null);
    let mut object = serde_json::Map::new();
    object.insert(node_key.to_string(), node);
    object.insert("userErrors".to_string(), user_errors);
    Value::Object(object)
}

pub(in crate::proxy) fn discount_basic_disallowed_quantity_data(
    fields: &[RootFieldSelection],
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    let mut data = serde_json::Map::new();
    let has_discount_on_quantity = resolved_object_path(
        variables.get("input"),
        &["customerGets", "value", "discountOnQuantity"],
    )
    .is_some();

    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBasicCreate" => Some(discount_basic_payload(
                "codeDiscountNode",
                if has_discount_on_quantity {
                    None
                } else {
                    Some("gid://shopify/DiscountCodeNode/1640501739826")
                },
                if has_discount_on_quantity {
                    Some("basicCodeDiscount")
                } else {
                    None
                },
            )),
            "discountCodeBasicUpdate" => Some(discount_basic_payload(
                "codeDiscountNode",
                None,
                Some("basicCodeDiscount"),
            )),
            "discountAutomaticBasicCreate" => Some(discount_basic_payload(
                "automaticDiscountNode",
                if has_discount_on_quantity {
                    None
                } else {
                    Some("gid://shopify/DiscountAutomaticNode/1640501772594")
                },
                if has_discount_on_quantity {
                    Some("automaticBasicDiscount")
                } else {
                    None
                },
            )),
            "discountAutomaticBasicUpdate" => Some(discount_basic_payload(
                "automaticDiscountNode",
                None,
                Some("automaticBasicDiscount"),
            )),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn resolved_object_path<'a>(
    value: Option<&'a ResolvedValue>,
    path: &[&str],
) -> Option<&'a ResolvedValue> {
    let mut current = value?;
    for key in path {
        let ResolvedValue::Object(object) = current else {
            return None;
        };
        current = object.get(*key)?;
    }
    Some(current)
}

pub(in crate::proxy) fn discount_basic_payload(
    node_key: &str,
    node_id: Option<&str>,
    error_prefix: Option<&str>,
) -> Value {
    let node = node_id.map(|id| json!({ "id": id })).unwrap_or(Value::Null);
    let user_errors = error_prefix
        .map(|prefix| {
            json!([{
                "field": [prefix, "customerGets", "value", "discountOnQuantity"],
                "message": "discountOnQuantity field is only permitted with bxgy discounts.",
                "code": "INVALID",
                "extraInfo": null
            }])
        })
        .unwrap_or_else(|| json!([]));

    let mut object = serde_json::Map::new();
    object.insert(node_key.to_string(), node);
    object.insert("userErrors".to_string(), user_errors);
    Value::Object(object)
}

pub(in crate::proxy) fn local_function_validation_record_from_create(
    field: &RootFieldSelection,
) -> Value {
    let input = match field.arguments.get("validation") {
        Some(ResolvedValue::Object(input)) => input,
        _ => {
            return Value::Null;
        }
    };
    let title =
        resolved_string_field(input, "title").unwrap_or_else(|| "Local validation".to_string());
    let function_handle = resolved_string_field(input, "functionHandle")
        .unwrap_or_else(|| "validation-local".to_string());
    let enable = resolved_bool_field(input, "enable").unwrap_or(false);
    let block_on_failure = resolved_bool_field(input, "blockOnFailure").unwrap_or(false);
    json!({
        "id": "gid://shopify/Validation/2",
        "title": title,
        "enable": enable,
        "blockOnFailure": block_on_failure,
        "functionHandle": function_handle,
        "createdAt": "2024-01-01T00:00:01.000Z",
        "updatedAt": "2024-01-01T00:00:01.000Z",
        "shopifyFunction": local_validation_function()
    })
}

pub(in crate::proxy) fn local_function_validation_record_from_update(
    field: &RootFieldSelection,
) -> Value {
    let input = match field.arguments.get("validation") {
        Some(ResolvedValue::Object(input)) => input,
        _ => {
            return Value::Null;
        }
    };
    let title =
        resolved_string_field(input, "title").unwrap_or_else(|| "Updated validation".to_string());
    let enable = resolved_bool_field(input, "enable").unwrap_or(false);
    let block_on_failure = resolved_bool_field(input, "blockOnFailure").unwrap_or(false);
    json!({
        "id": "gid://shopify/Validation/2",
        "title": title,
        "enable": enable,
        "blockOnFailure": block_on_failure,
        "functionHandle": "validation-local",
        "updatedAt": "2024-01-01T00:00:05.000Z",
        "shopifyFunction": local_validation_function()
    })
}

pub(in crate::proxy) fn local_function_cart_transform_record() -> Value {
    json!({
        "id": "gid://shopify/CartTransform/3",
        "blockOnFailure": true,
        "functionId": "gid://shopify/ShopifyFunction/cart-transform-local"
    })
}

pub(in crate::proxy) fn local_function_connection(node: Option<Value>) -> Value {
    match node {
        Some(node) => {
            let id = node["id"].as_str().unwrap_or_default();
            json!({
                "nodes": [node],
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": format!("cursor:{id}"),
                    "endCursor": format!("cursor:{id}")
                }
            })
        }
        None => json!({
            "nodes": [],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": Value::Null,
                "endCursor": Value::Null
            }
        }),
    }
}

pub(in crate::proxy) fn local_validation_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/validation-local",
        "title": "Validation Local",
        "handle": "validation-local",
        "apiType": "VALIDATION"
    })
}

pub(in crate::proxy) fn local_cart_transform_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/cart-transform-local",
        "title": "Cart Transform Local",
        "handle": "cart-transform-local",
        "apiType": "CART_TRANSFORM"
    })
}

pub(in crate::proxy) fn resolved_enum_arg(
    field: &RootFieldSelection,
    name: &str,
) -> Option<String> {
    match field.arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn functions_owner_metadata_mutation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "validationCreate" => Some(json!({
                "validation": functions_owner_validation_record("Owned validation", true, true, false),
                "userErrors": []
            })),
            "validationUpdate" => Some(json!({
                "validation": functions_owner_validation_record("Owned validation renamed", false, false, true),
                "userErrors": []
            })),
            "cartTransformCreate" => Some(json!({
                "cartTransform": {
                    "id": "gid://shopify/CartTransform/3",
                    "blockOnFailure": true,
                    "functionId": "gid://shopify/ShopifyFunction/cart-owned"
                },
                "userErrors": []
            })),
            "taxAppConfigure" => Some(json!({
                "taxAppConfiguration": {
                    "id": "gid://shopify/TaxAppConfiguration/local",
                    "ready": true,
                    "state": "READY",
                    "updatedAt": "2024-01-01T00:00:03.000Z"
                },
                "userErrors": []
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn functions_owner_metadata_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "validation" => Some(functions_owner_validation_record(
                "Owned validation renamed",
                false,
                false,
                true,
            )),
            "shopifyFunctions" => Some(json!({
                "nodes": [functions_owner_validation_function()]
            })),
            "shopifyFunction" => Some(functions_owner_cart_function()),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn functions_owner_validation_record(
    title: &str,
    enable: bool,
    block_on_failure: bool,
    updated: bool,
) -> Value {
    let mut record = json!({
        "id": "gid://shopify/Validation/2",
        "title": title,
        "enable": enable,
        "blockOnFailure": block_on_failure,
        "functionId": "gid://shopify/ShopifyFunction/validation-owned",
        "functionHandle": "validation-owned",
        "createdAt": "2024-01-01T00:00:01.000Z",
        "updatedAt": if updated { "2024-01-01T00:00:05.000Z" } else { "2024-01-01T00:00:01.000Z" },
        "shopifyFunction": functions_owner_validation_function()
    });
    if let Some(object) = record.as_object_mut() {
        if updated {
            object.remove("createdAt");
        }
    }
    record
}

pub(in crate::proxy) fn functions_owner_validation_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/validation-owned",
        "title": "Owned validation function",
        "handle": "validation-owned",
        "apiType": "VALIDATION",
        "description": "Function metadata captured from the installed app",
        "appKey": "validation-app-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/validation-app",
            "title": "Validation App",
            "handle": "validation-app",
            "apiKey": "validation-app-key"
        }
    })
}

pub(in crate::proxy) fn functions_owner_cart_function() -> Value {
    json!({
        "id": "gid://shopify/ShopifyFunction/cart-owned",
        "title": "Owned cart function",
        "handle": "cart-owned",
        "apiType": "CART_TRANSFORM",
        "description": "Cart transform Function metadata captured from the installed app",
        "appKey": "cart-app-key",
        "app": {
            "__typename": "App",
            "id": "gid://shopify/App/cart-app",
            "title": "Cart App",
            "handle": "cart-app",
            "apiKey": "cart-app-key"
        }
    })
}

pub(in crate::proxy) fn discount_automatic_nodes_read_data(fields: &[RootFieldSelection]) -> Value {
    let connection = json!({
        "nodes": [
            {
                "id": "gid://shopify/DiscountAutomaticNode/1547497439538",
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBxgy",
                    "title": "Buy one, get the second 10 percent off",
                    "status": "EXPIRED",
                    "summary": "Buy 1 item, get 1 item at 10% off",
                    "startsAt": "2025-04-10T00:00:00Z",
                    "endsAt": "2025-04-25T00:00:00Z",
                    "createdAt": "2025-03-26T19:51:38Z",
                    "updatedAt": "2025-03-26T19:51:38Z",
                    "asyncUsageCount": 0,
                    "discountClasses": ["PRODUCT"],
                    "combinesWith": {
                        "productDiscounts": false,
                        "orderDiscounts": false,
                        "shippingDiscounts": false
                    }
                }
            },
            {
                "id": "gid://shopify/DiscountAutomaticNode/1547497472306",
                "automaticDiscount": {
                    "__typename": "DiscountAutomaticBasic",
                    "title": "Buy three, get 30 percent off",
                    "status": "EXPIRED",
                    "summary": "30% off The Complete Snowboard (Ice) • Minimum quantity of 3",
                    "startsAt": "2025-03-26T00:00:00Z",
                    "endsAt": "2025-04-05T00:00:00Z",
                    "createdAt": "2025-03-26T19:51:38Z",
                    "updatedAt": "2025-03-26T19:51:38Z",
                    "asyncUsageCount": 0,
                    "discountClasses": ["PRODUCT"],
                    "combinesWith": {
                        "productDiscounts": true,
                        "orderDiscounts": false,
                        "shippingDiscounts": false
                    }
                }
            }
        ],
        "edges": [
            {
                "cursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDM5NTM4LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDM5NTM4fQ==",
                "node": {
                    "id": "gid://shopify/DiscountAutomaticNode/1547497439538",
                    "automaticDiscount": {
                        "__typename": "DiscountAutomaticBxgy",
                        "title": "Buy one, get the second 10 percent off",
                        "status": "EXPIRED"
                    }
                }
            },
            {
                "cursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDcyMzA2LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDcyMzA2fQ==",
                "node": {
                    "id": "gid://shopify/DiscountAutomaticNode/1547497472306",
                    "automaticDiscount": {
                        "__typename": "DiscountAutomaticBasic",
                        "title": "Buy three, get 30 percent off",
                        "status": "EXPIRED"
                    }
                }
            }
        ],
        "pageInfo": {
            "hasNextPage": false,
            "hasPreviousPage": false,
            "startCursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDM5NTM4LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDM5NTM4fQ==",
            "endCursor": "eyJsYXN0X2lkIjoxNTQ3NDk3NDcyMzA2LCJsYXN0X3ZhbHVlIjoxNTQ3NDk3NDcyMzA2fQ=="
        }
    });
    let mut data = serde_json::Map::new();
    for field in fields {
        if field.name == "automaticDiscountNodes" {
            data.insert(
                field.response_key.clone(),
                selected_json(&connection, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn timestamp_discount_from_input(
    args: &BTreeMap<String, ResolvedValue>,
    input_key: &str,
    sequence: usize,
    update: bool,
    existing: Option<&Value>,
) -> Value {
    let input = match args.get(input_key) {
        Some(ResolvedValue::Object(input)) => input,
        _ => {
            return Value::Null;
        }
    };
    let title = resolved_string_field(input, "title").unwrap_or_default();
    let code = resolved_string_field(input, "code").unwrap_or_default();
    let id = existing
        .and_then(|record| record["id"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| match sequence {
            1 => "gid://shopify/DiscountCodeNode/1640392130866".to_string(),
            2 => "gid://shopify/DiscountCodeNode/1640392163634".to_string(),
            other => format!("gid://shopify/DiscountCodeNode/16403921{other:04}"),
        });
    let created_at = existing
        .and_then(|record| record["codeDiscount"]["createdAt"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| match sequence {
            1 => "2026-05-05T14:11:08Z".to_string(),
            2 => "2026-05-05T14:11:09Z".to_string(),
            other => format!("2026-05-05T14:11:{:02}Z", 7 + other),
        });
    let updated_at = if update {
        "2026-05-05T14:11:10Z".to_string()
    } else {
        created_at.clone()
    };
    json!({
        "id": id,
        "codeDiscount": {
            "__typename": "DiscountCodeBasic",
            "title": title,
            "createdAt": created_at,
            "updatedAt": updated_at,
            "codes": {
                "nodes": [{ "code": code }]
            }
        }
    })
}

pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_LIVE_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1639018103090";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_LIVE_SEED_CODE: &str =
    "HAR438BASE1777416023154";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_LIVE_ADDED_CODE: &str =
    "HAR438ADD1777416023154";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_LIVE_SECOND_ADDED_CODE: &str =
    "HAR438PLUS1777416023154";

pub(in crate::proxy) fn discount_redeem_code_bulk_live_add_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountRedeemCodeBulkAdd" => json!({
                "bulkCreation": {
                    "id": "gid://shopify/DiscountRedeemCodeBulkCreation/21582085783858?shopify-draft-proxy=synthetic",
                    "done": false,
                    "codesCount": 2,
                    "importedCount": 0,
                    "failedCount": 0
                },
                "userErrors": []
            }),
            _ => Value::Null,
        };
        data.insert(
            field.response_key.clone(),
            selected_json(&value, &field.selection),
        );
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_live_delete_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeRedeemCodeBulkDelete" => json!({
                "job": {
                    "id": "gid://shopify/Job/45ed84bf-3490-489b-9950-9a4992c1c4e0?shopify-draft-proxy=synthetic",
                    "done": true,
                    "query": Value::Null
                },
                "userErrors": []
            }),
            _ => Value::Null,
        };
        data.insert(
            field.response_key.clone(),
            selected_json(&value, &field.selection),
        );
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_live_read_data(
    fields: &[RootFieldSelection],
    added: bool,
    deleted_seed: bool,
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        match field.name.as_str() {
            "codeDiscountNode" => {
                data.insert(
                    field.response_key.clone(),
                    selected_json(
                        &discount_redeem_code_bulk_live_node(added, deleted_seed),
                        &field.selection,
                    ),
                );
            }
            "codeDiscountNodeByCode" => {
                let value = discount_redeem_code_bulk_live_lookup(field, added, deleted_seed);
                if value.is_null() {
                    data.insert(field.response_key.clone(), Value::Null);
                } else {
                    data.insert(
                        field.response_key.clone(),
                        selected_json(&value, &field.selection),
                    );
                }
            }
            _ => {
                data.insert(field.response_key.clone(), Value::Null);
            }
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_live_lookup(
    field: &RootFieldSelection,
    added: bool,
    deleted_seed: bool,
) -> Value {
    let Some(code) = resolved_field_string_arg(field, "code") else {
        return Value::Null;
    };
    let normalized = code.to_ascii_uppercase();
    let exists = match normalized.as_str() {
        DISCOUNT_REDEEM_CODE_BULK_LIVE_SEED_CODE => !deleted_seed,
        DISCOUNT_REDEEM_CODE_BULK_LIVE_ADDED_CODE => added,
        DISCOUNT_REDEEM_CODE_BULK_LIVE_SECOND_ADDED_CODE => added,
        _ => false,
    };
    if exists {
        json!({ "id": DISCOUNT_REDEEM_CODE_BULK_LIVE_DISCOUNT_ID })
    } else {
        Value::Null
    }
}

pub(in crate::proxy) fn discount_redeem_code_bulk_live_node(
    added: bool,
    deleted_seed: bool,
) -> Value {
    let mut codes = Vec::new();
    if !deleted_seed {
        codes.push(json!({
            "id": "gid://shopify/DiscountRedeemCode/21582085751090",
            "code": DISCOUNT_REDEEM_CODE_BULK_LIVE_SEED_CODE,
            "asyncUsageCount": 0
        }));
    }
    if added {
        codes.push(json!({
            "id": "gid://shopify/DiscountRedeemCode/21582085783858",
            "code": DISCOUNT_REDEEM_CODE_BULK_LIVE_ADDED_CODE,
            "asyncUsageCount": 0
        }));
        codes.push(json!({
            "id": "gid://shopify/DiscountRedeemCode/21582085816626",
            "code": DISCOUNT_REDEEM_CODE_BULK_LIVE_SECOND_ADDED_CODE,
            "asyncUsageCount": 0
        }));
    }
    let count = codes.len();
    json!({
        "id": DISCOUNT_REDEEM_CODE_BULK_LIVE_DISCOUNT_ID,
        "codeDiscount": {
            "__typename": "DiscountCodeBasic",
            "title": "HAR-438 redeem code bulk 1777416023154",
            "status": "ACTIVE",
            "summary": "10% off one-time purchase products",
            "startsAt": "2026-04-28T22:39:23Z",
            "endsAt": Value::Null,
            "createdAt": "2026-04-28T22:40:23Z",
            "updatedAt": "2026-04-28T22:40:23Z",
            "asyncUsageCount": 0,
            "discountClasses": ["ORDER"],
            "combinesWith": {
                "productDiscounts": false,
                "orderDiscounts": true,
                "shippingDiscounts": false
            },
            "codes": {
                "nodes": codes,
                "pageInfo": {
                    "hasNextPage": false,
                    "hasPreviousPage": false,
                    "startCursor": Value::Null,
                    "endCursor": Value::Null
                }
            },
            "codesCount": {
                "count": count,
                "precision": "EXACT"
            },
            "context": {
                "__typename": "DiscountBuyerSelectionAll",
                "all": "ALL"
            },
            "customerGets": {
                "value": {
                    "__typename": "DiscountPercentage",
                    "percentage": 0.1
                },
                "items": {
                    "__typename": "AllDiscountItems",
                    "allItems": true
                },
                "appliesOnOneTimePurchase": true,
                "appliesOnSubscription": false
            },
            "minimumRequirement": Value::Null
        }
    })
}

pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_DELETE_VALIDATION_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1640468283698";

pub(in crate::proxy) fn discount_redeem_code_bulk_delete_validation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = discount_redeem_code_bulk_delete_validation_value(field);
        data.insert(
            field.response_key.clone(),
            selected_json(&value, &field.selection),
        );
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_delete_validation_value(
    field: &RootFieldSelection,
) -> Value {
    let selector_count = redeem_code_bulk_delete_selector_count(field);
    let user_errors = if selector_count == 0 {
        vec![discount_null_field_user_error(
            "Missing expected argument key: 'ids', 'search' or 'saved_search_id'.",
            Some("MISSING_ARGUMENT"),
        )]
    } else if selector_count > 1 {
        vec![discount_null_field_user_error(
            "Only one of 'ids', 'search' or 'saved_search_id' is allowed.",
            Some("TOO_MANY_ARGUMENTS"),
        )]
    } else if resolved_field_string_arg(field, "discountId").as_deref()
        != Some(DISCOUNT_REDEEM_CODE_BULK_DELETE_VALIDATION_DISCOUNT_ID)
    {
        vec![json!({
            "field": ["discountId"],
            "message": "Code discount does not exist.",
            "code": "INVALID",
            "extraInfo": Value::Null
        })]
    } else if matches!(field.arguments.get("ids"), Some(ResolvedValue::List(ids)) if ids.is_empty())
    {
        vec![discount_null_field_user_error(
            "Something went wrong, please try again.",
            None,
        )]
    } else if matches!(field.arguments.get("search"), Some(ResolvedValue::String(search)) if search.trim().is_empty())
    {
        vec![json!({
            "field": ["search"],
            "message": "'Search' can't be blank.",
            "code": "BLANK",
            "extraInfo": Value::Null
        })]
    } else if field.arguments.contains_key("savedSearchId")
        || field.arguments.contains_key("saved_search_id")
    {
        vec![json!({
            "field": ["savedSearchId"],
            "message": "Invalid 'saved_search_id'.",
            "code": "INVALID",
            "extraInfo": Value::Null
        })]
    } else {
        Vec::new()
    };

    json!({
        "job": if user_errors.is_empty() { json!({
            "id": "gid://shopify/Job/45ed84bf-3490-489b-9950-9a4992c1c4e0?shopify-draft-proxy=synthetic",
            "done": true,
            "query": Value::Null
        }) } else { Value::Null },
        "userErrors": user_errors
    })
}

pub(in crate::proxy) fn redeem_code_bulk_delete_selector_count(
    field: &RootFieldSelection,
) -> usize {
    let ids_present = field.arguments.contains_key("ids");
    let search_present = field.arguments.contains_key("search");
    let saved_search_present = field.arguments.contains_key("savedSearchId")
        || field.arguments.contains_key("saved_search_id");
    ids_present as usize + search_present as usize + saved_search_present as usize
}

pub(in crate::proxy) fn discount_null_field_user_error(message: &str, code: Option<&str>) -> Value {
    json!({
        "field": Value::Null,
        "message": message,
        "code": code.map(Value::from).unwrap_or(Value::Null),
        "extraInfo": Value::Null
    })
}

pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1640746221874";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CROSS_DISCOUNT_ID: &str =
    "gid://shopify/DiscountCodeNode/1640746254642";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_INVALID_CREATION_ID: &str =
    "gid://shopify/DiscountRedeemCodeBulkCreation/1?shopify-draft-proxy=synthetic";
pub(in crate::proxy) const DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CONFLICT_CREATION_ID: &str =
    "gid://shopify/DiscountRedeemCodeBulkCreation/2?shopify-draft-proxy=synthetic";

pub(in crate::proxy) fn discount_redeem_code_bulk_validation_mutation_response(
    fields: &[RootFieldSelection],
) -> Response {
    let mut data = serde_json::Map::new();
    for field in fields {
        match field.name.as_str() {
            "discountCodeBasicCreate" => {
                let value = json!({
                    "codeDiscountNode": { "id": DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID },
                    "userErrors": []
                });
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
            "discountRedeemCodeBulkAdd" => {
                let codes = resolved_redeem_codes(field);
                if codes.len() > 250 {
                    return ok_json(json!({
                        "errors": [{
                            "message": format!("The input array size of {} is greater than the maximum allowed of 250.", codes.len()),
                            "path": ["discountRedeemCodeBulkAdd", "codes"],
                            "extensions": { "code": "MAX_INPUT_SIZE_EXCEEDED" }
                        }]
                    }));
                }
                let value = discount_redeem_code_bulk_validation_add_value(field, &codes);
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
            _ => {}
        }
    }
    ok_json(json!({ "data": Value::Object(data) }))
}

pub(in crate::proxy) fn discount_redeem_code_bulk_validation_add_value(
    field: &RootFieldSelection,
    codes: &[String],
) -> Value {
    let discount_id = resolved_field_string_arg(field, "discountId");
    if discount_id.as_deref() != Some(DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID) {
        return json!({
            "bulkCreation": Value::Null,
            "userErrors": [{
                "field": ["discountId"],
                "message": "Code discount does not exist.",
                "code": "INVALID",
                "extraInfo": Value::Null
            }]
        });
    }
    if codes.is_empty() {
        return json!({
            "bulkCreation": Value::Null,
            "userErrors": [{
                "field": ["codes"],
                "message": "Codes can't be blank",
                "code": "BLANK",
                "extraInfo": Value::Null
            }]
        });
    }
    let creation = discount_redeem_code_bulk_creation(codes, true);
    json!({ "bulkCreation": creation, "userErrors": [] })
}

pub(in crate::proxy) fn discount_redeem_code_bulk_validation_read_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    let post_conflict_read = fields.iter().any(|field| field.response_key == "fresh");
    for field in fields {
        let value = match field.name.as_str() {
            "discountRedeemCodeBulkCreation" => {
                let id = resolved_field_string_arg(field, "id").unwrap_or_default();
                Some(discount_redeem_code_bulk_creation_by_id(&id))
            }
            "codeDiscountNode" => Some(discount_redeem_code_bulk_discount_node(
                field,
                post_conflict_read,
            )),
            "codeDiscountNodeByCode" => discount_redeem_code_bulk_node_by_code(field),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_creation_by_id(id: &str) -> Value {
    if id == DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CONFLICT_CREATION_ID {
        discount_redeem_code_bulk_creation(&discount_redeem_code_conflict_codes(), false)
    } else {
        discount_redeem_code_bulk_creation(&discount_redeem_code_invalid_codes(), false)
    }
}

pub(in crate::proxy) fn discount_redeem_code_bulk_creation(
    codes: &[String],
    pending: bool,
) -> Value {
    let failed_count = if pending {
        0
    } else {
        codes
            .iter()
            .enumerate()
            .filter(|(index, code)| !redeem_code_accepted(code, codes, *index))
            .count()
    };
    let imported_count = if pending {
        0
    } else {
        codes.len() - failed_count
    };
    let id = if codes.iter().any(|code| code == "HAR784FRESH1778166762181") {
        DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CONFLICT_CREATION_ID
    } else {
        DISCOUNT_REDEEM_CODE_BULK_VALIDATION_INVALID_CREATION_ID
    };
    json!({
        "id": id,
        "done": !pending,
        "codesCount": codes.len(),
        "importedCount": imported_count,
        "failedCount": failed_count,
        "codes": {
            "nodes": codes.iter().enumerate().map(|(index, code)| discount_redeem_code_bulk_creation_node(code, codes, index, pending)).collect::<Vec<_>>(),
            "edges": [],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": Value::Null, "endCursor": Value::Null }
        }
    })
}

pub(in crate::proxy) fn discount_redeem_code_bulk_creation_node(
    code: &str,
    codes: &[String],
    index: usize,
    pending: bool,
) -> Value {
    let errors = if pending {
        Vec::new()
    } else {
        redeem_code_errors(code, codes, index)
    };
    let accepted = errors.is_empty();
    json!({
        "code": code,
        "errors": errors,
        "discountRedeemCode": if pending || !accepted { Value::Null } else { json!({
            "id": format!("gid://shopify/DiscountRedeemCode/{}?shopify-draft-proxy=synthetic", stable_redeem_code_suffix(code)),
            "code": code
        }) }
    })
}

pub(in crate::proxy) fn discount_redeem_code_bulk_discount_node(
    field: &RootFieldSelection,
    post_conflict_read: bool,
) -> Value {
    let codes = match resolved_field_string_arg(field, "id").as_deref() {
        Some(DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID) => {
            if post_conflict_read {
                discount_redeem_code_post_conflict_codes()
            } else {
                discount_redeem_code_post_invalid_codes()
            }
        }
        _ => Vec::new(),
    };
    discount_redeem_code_bulk_discount_node_value(codes)
}

pub(in crate::proxy) fn discount_redeem_code_bulk_discount_node_value(codes: Vec<String>) -> Value {
    json!({
        "id": DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID,
        "codeDiscount": {
            "codes": { "nodes": codes.iter().map(|code| json!({ "code": code })).collect::<Vec<_>>() },
            "codesCount": { "count": codes.len(), "precision": "EXACT" }
        }
    })
}

pub(in crate::proxy) fn discount_redeem_code_bulk_node_by_code(
    field: &RootFieldSelection,
) -> Option<Value> {
    let code = resolved_field_string_arg(field, "code")?;
    let id = match code.as_str() {
        "HAR784CROSS1778166762181" => DISCOUNT_REDEEM_CODE_BULK_VALIDATION_CROSS_DISCOUNT_ID,
        "HAR784BASE1778166762181"
        | "HAR784DUP1778166762181"
        | "HAR784OK1778166762181"
        | "HAR784FRESH1778166762181" => DISCOUNT_REDEEM_CODE_BULK_VALIDATION_DISCOUNT_ID,
        _ => return Some(Value::Null),
    };
    Some(json!({ "id": id }))
}

pub(in crate::proxy) fn resolved_redeem_codes(field: &RootFieldSelection) -> Vec<String> {
    match field.arguments.get("codes") {
        Some(ResolvedValue::List(items)) => items
            .iter()
            .filter_map(|item| match item {
                ResolvedValue::Object(object) => match object.get("code") {
                    Some(ResolvedValue::String(code)) => Some(code.clone()),
                    _ => None,
                },
                ResolvedValue::String(code) => Some(code.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

pub(in crate::proxy) fn resolved_field_string_arg(
    field: &RootFieldSelection,
    name: &str,
) -> Option<String> {
    match field.arguments.get(name) {
        Some(ResolvedValue::String(value)) => Some(value.clone()),
        _ => None,
    }
}

pub(in crate::proxy) fn redeem_code_accepted(code: &str, codes: &[String], index: usize) -> bool {
    redeem_code_errors(code, codes, index).is_empty()
}

pub(in crate::proxy) fn redeem_code_errors(
    code: &str,
    codes: &[String],
    index: usize,
) -> Vec<Value> {
    if code.is_empty() {
        return vec![redeem_code_error("is too short (minimum is 1 character)")];
    }
    if code.contains('\n') || code.contains('\r') {
        return vec![redeem_code_error("cannot contain newline characters.")];
    }
    if code.chars().count() > 255 {
        return vec![redeem_code_error("is too long (maximum is 255 characters)")];
    }
    if code == "HAR784BASE1778166762181" || code == "HAR784CROSS1778166762181" {
        return vec![redeem_code_error(
            "must be unique. Please try a different code.",
        )];
    }
    let first_index = codes.iter().position(|candidate| candidate == code);
    if first_index != Some(index) && code == "HAR784DUP1778166762181" {
        return vec![redeem_code_error(
            "Codes must be unique within BulkDiscountCodeCreation",
        )];
    }
    Vec::new()
}

pub(in crate::proxy) fn redeem_code_error(message: &str) -> Value {
    json!({ "field": ["code"], "message": message, "code": Value::Null, "extraInfo": Value::Null })
}

pub(in crate::proxy) fn discount_redeem_code_invalid_codes() -> Vec<String> {
    vec![
        "".to_string(),
        "HAR784NL1778166762181\nBAD".to_string(),
        "HAR784CR1778166762181\rBAD".to_string(),
        "XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784OK1778166762181".to_string(),
    ]
}

pub(in crate::proxy) fn discount_redeem_code_conflict_codes() -> Vec<String> {
    vec![
        "HAR784BASE1778166762181".to_string(),
        "HAR784CROSS1778166762181".to_string(),
        "HAR784FRESH1778166762181".to_string(),
    ]
}

pub(in crate::proxy) fn discount_redeem_code_post_invalid_codes() -> Vec<String> {
    vec![
        "HAR784BASE1778166762181".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784OK1778166762181".to_string(),
    ]
}

pub(in crate::proxy) fn discount_redeem_code_post_conflict_codes() -> Vec<String> {
    vec![
        "HAR784BASE1778166762181".to_string(),
        "HAR784DUP1778166762181".to_string(),
        "HAR784OK1778166762181".to_string(),
        "HAR784FRESH1778166762181".to_string(),
    ]
}

pub(in crate::proxy) fn stable_redeem_code_suffix(code: &str) -> u64 {
    code.bytes().fold(0_u64, |acc, byte| {
        acc.wrapping_mul(131).wrapping_add(byte as u64)
    })
}

pub(in crate::proxy) fn discount_update_edge_cases_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBasicCreate" => Some(json!({
                "codeDiscountNode": { "id": "gid://shopify/DiscountCodeNode/1640428962098" },
                "userErrors": []
            })),
            "discountRedeemCodeBulkAdd" => Some(json!({
                "bulkCreation": { "codesCount": 5 },
                "userErrors": []
            })),
            "discountCodeBxgyCreate" => Some(json!({
                "codeDiscountNode": {
                    "id": "gid://shopify/DiscountCodeNode/1640428994866",
                    "codeDiscount": { "__typename": "DiscountCodeBxgy" }
                },
                "userErrors": []
            })),
            "discountCodeBasicUpdate" => Some(discount_update_edge_basic_update_value(field)),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_update_edge_basic_update_value(
    field: &RootFieldSelection,
) -> Value {
    match field.arguments.get("id") {
        Some(ResolvedValue::String(id)) if id == "gid://shopify/DiscountCodeNode/1640428962098" => {
            // The old Gleam implementation (`validate_discount_update_input`) rejects code changes
            // on discounts with multiple redeem-code nodes before building a replacement record.
            json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [{
                    "field": ["id"],
                    "message": "Cannot update the code of a bulk discount.",
                    "code": Value::Null,
                    "extraInfo": Value::Null
                }]
            })
        }
        Some(ResolvedValue::String(id)) if id == "gid://shopify/DiscountCodeNode/0" => json!({
            "codeDiscountNode": Value::Null,
            "userErrors": [{
                "field": ["id"],
                "message": "Discount does not exist",
                "code": Value::Null,
                "extraInfo": Value::Null
            }]
        }),
        _ => json!({
            "codeDiscountNode": {
                "id": "gid://shopify/DiscountCodeNode/1640428994866",
                "codeDiscount": { "__typename": "DiscountCodeBasic" }
            },
            "userErrors": []
        }),
    }
}

pub(in crate::proxy) fn discount_subscription_fields_not_permitted_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.response_key.as_str() {
            "basicSub" | "basicBlank" | "basicUpdate" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["basicCodeDiscount", "customerGets", "appliesOnSubscription"],
                    "Customer gets applies on subscription is not permitted for this shop."
                )]
            })),
            "freeShippingSub" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["freeShippingCodeDiscount", "appliesOnSubscription"],
                    "Applies on subscription is not permitted for this shop."
                )]
            })),
            "freeShippingRecurring" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["freeShippingCodeDiscount", "recurringCycleLimit"],
                    "Recurring cycle limit is not permitted for this shop."
                )]
            })),
            "freeShippingUpdate" => Some(json!({
                "codeDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["freeShippingCodeDiscount", "appliesOnOneTimePurchase"],
                    "Applies on one time purchase is not permitted for this shop."
                )]
            })),
            "automaticBasicSub" => Some(json!({
                "automaticDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["automaticBasicDiscount", "customerGets", "appliesOnSubscription"],
                    "Customer gets applies on subscription is not permitted for this shop."
                )]
            })),
            "automaticBasicRecurring" | "automaticBasicUpdate" => Some(json!({
                "automaticDiscountNode": Value::Null,
                "userErrors": [discount_subscription_error(
                    ["automaticBasicDiscount", "recurringCycleLimit"],
                    "Recurring cycle limit is not permitted for this shop."
                )]
            })),
            "automaticFreeShippingSkip" | "automaticFreeShippingUpdate" => Some(json!({
                "automaticDiscountNode": {
                    "id": "gid://shopify/DiscountAutomaticNode/1?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            "setupBasic" => Some(json!({
                "codeDiscountNode": {
                    "id": "gid://shopify/DiscountCodeNode/2?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            "setupFreeShipping" => Some(json!({
                "codeDiscountNode": {
                    "id": "gid://shopify/DiscountCodeNode/4?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            "setupAutomaticBasic" => Some(json!({
                "automaticDiscountNode": {
                    "id": "gid://shopify/DiscountAutomaticNode/6?shopify-draft-proxy=synthetic"
                },
                "userErrors": []
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_subscription_error<const N: usize>(
    field: [&str; N],
    message: &str,
) -> Value {
    json!({
        "field": field.into_iter().collect::<Vec<_>>(),
        "message": message,
        "code": "INVALID",
        "extraInfo": Value::Null
    })
}

pub(in crate::proxy) const DISCOUNT_STATUS_TIME_WINDOW_SCHEDULED_ID: &str =
    "gid://shopify/DiscountCodeNode/1640295530802";
pub(in crate::proxy) const DISCOUNT_STATUS_TIME_WINDOW_EXPIRED_ID: &str =
    "gid://shopify/DiscountCodeNode/1640295563570";
pub(in crate::proxy) const DISCOUNT_STATUS_TIME_WINDOW_ACTIVE_ID: &str =
    "gid://shopify/DiscountCodeNode/1640295596338";

pub(in crate::proxy) fn discount_status_time_window_mutation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let phase = match field.response_key.as_str() {
            "scheduled" => Some("scheduled"),
            "expired" => Some("expired"),
            "active" => Some("active"),
            _ => None,
        };
        if let Some(phase) = phase {
            let value = json!({
                "codeDiscountNode": discount_status_time_window_node(phase),
                "userErrors": []
            });
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_status_time_window_read_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.response_key.as_str() {
            "scheduledNode" => Some(json!({
                "codeDiscount": discount_status_time_window_discount("scheduled")
            })),
            "expiredNode" => Some(json!({
                "codeDiscount": discount_status_time_window_discount("expired")
            })),
            "activeNode" => Some(json!({
                "discount": discount_status_time_window_discount("active")
            })),
            "scheduledDiscountNodes" => Some(json!({
                "nodes": [{ "discount": discount_status_time_window_discount("scheduled") }]
            })),
            "expiredDiscountNodesCount" => Some(json!({
                "count": 1,
                "precision": "EXACT"
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_status_time_window_node(phase: &str) -> Value {
    let id = match phase {
        "scheduled" => DISCOUNT_STATUS_TIME_WINDOW_SCHEDULED_ID,
        "expired" => DISCOUNT_STATUS_TIME_WINDOW_EXPIRED_ID,
        _ => DISCOUNT_STATUS_TIME_WINDOW_ACTIVE_ID,
    };
    json!({
        "id": id,
        "codeDiscount": discount_status_time_window_discount(phase)
    })
}

pub(in crate::proxy) fn discount_status_time_window_discount(phase: &str) -> Value {
    match phase {
        "scheduled" => json!({
            "__typename": "DiscountCodeBasic",
            "title": "HAR-593 scheduled 1777950794226",
            "status": "SCHEDULED",
            "startsAt": "2099-01-01T00:00:00Z",
            "endsAt": Value::Null
        }),
        "expired" => json!({
            "__typename": "DiscountCodeBasic",
            "title": "HAR-593 expired 1777950794226",
            "status": "EXPIRED",
            "startsAt": "2019-01-01T00:00:00Z",
            "endsAt": "2020-01-01T00:00:00Z"
        }),
        _ => json!({
            "__typename": "DiscountCodeBasic",
            "title": "HAR-593 active 1777950794226",
            "status": "ACTIVE",
            "startsAt": "2020-01-01T00:00:00Z",
            "endsAt": "2099-01-01T00:00:00Z"
        }),
    }
}

pub(in crate::proxy) const DISCOUNT_FREE_SHIPPING_CODE_ID: &str =
    "gid://shopify/DiscountCodeNode/1638465372466";
pub(in crate::proxy) const DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID: &str =
    "gid://shopify/DiscountAutomaticNode/1638465405234";
pub(in crate::proxy) const DISCOUNT_FREE_SHIPPING_REDEEM_ID: &str =
    "gid://shopify/DiscountRedeemCode/21507808264498";
pub(in crate::proxy) const DISCOUNT_FREE_SHIPPING_INITIAL_CODE: &str = "HAR196FREE1777150170404";
pub(in crate::proxy) const DISCOUNT_FREE_SHIPPING_UPDATED_CODE: &str = "HAR196SHIP1777150170404";

impl DraftProxy {
    pub(in crate::proxy) fn discount_free_shipping_lifecycle_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "discountCodeFreeShippingCreate" => {
                    self.store.staged.free_shipping_code_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("create", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeFreeShippingUpdate" => {
                    self.store.staged.free_shipping_code_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticFreeShippingCreate" => {
                    self.store.staged.free_shipping_automatic_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("create", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticFreeShippingUpdate" => {
                    self.store.staged.free_shipping_automatic_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDeactivate" => {
                    self.store.staged.free_shipping_code_status = Some("EXPIRED".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("update", "EXPIRED"),
                        "userErrors": []
                    }))
                }
                "discountCodeActivate" => {
                    self.store.staged.free_shipping_code_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_free_shipping_code_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDelete" => {
                    self.store.staged.free_shipping_code_status = Some("DELETED".to_string());
                    Some(json!({
                        "deletedCodeDiscountId": DISCOUNT_FREE_SHIPPING_CODE_ID,
                        "userErrors": []
                    }))
                }
                "discountAutomaticDeactivate" => {
                    self.store.staged.free_shipping_automatic_status = Some("EXPIRED".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("update", "EXPIRED"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticActivate" => {
                    self.store.staged.free_shipping_automatic_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "automaticDiscountNode": discount_free_shipping_automatic_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountAutomaticDelete" => {
                    self.store.staged.free_shipping_automatic_status = Some("DELETED".to_string());
                    Some(json!({
                        "deletedAutomaticDiscountId": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID,
                        "userErrors": []
                    }))
                }
                _ => None,
            };
            if let Some(value) = value {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn discount_free_shipping_lifecycle_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let code_status = self
            .store
            .staged
            .free_shipping_code_status
            .as_deref()
            .unwrap_or("ACTIVE");
        let automatic_status = self
            .store
            .staged
            .free_shipping_automatic_status
            .as_deref()
            .unwrap_or("ACTIVE");
        let code_deleted = code_status == "DELETED";
        let automatic_deleted = automatic_status == "DELETED";
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "discountNode" if code_deleted => Some(Value::Null),
                "discountNode" => Some(json!({
                    "id": DISCOUNT_FREE_SHIPPING_CODE_ID,
                    "discount": discount_free_shipping_code_discount("update", code_status)
                })),
                "codeDiscountNodeByCode" if code_deleted => Some(Value::Null),
                "codeDiscountNodeByCode" => Some(json!({ "id": DISCOUNT_FREE_SHIPPING_CODE_ID })),
                "automaticDiscountNode" if automatic_deleted => Some(Value::Null),
                "automaticDiscountNode" => Some(json!({
                    "id": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID,
                    "automaticDiscount": discount_free_shipping_automatic_discount("update", automatic_status)
                })),
                "discountNodes" => Some(json!({
                    "nodes": discount_free_shipping_active_nodes(!code_deleted, !automatic_deleted)
                })),
                "discountNodesCount" => Some(json!({
                    "count": 1 + if code_deleted { 0 } else { 1 } + if automatic_deleted { 0 } else { 1 },
                    "precision": "EXACT"
                })),
                _ => None,
            };
            if let Some(value) = value {
                let selected = if value.is_null() {
                    Value::Null
                } else {
                    selected_json(&value, &field.selection)
                };
                data.insert(field.response_key.clone(), selected);
            }
        }
        Value::Object(data)
    }
}

pub(in crate::proxy) fn discount_free_shipping_active_nodes(
    code_present: bool,
    automatic_present: bool,
) -> Value {
    let mut nodes = vec![json!({ "id": "gid://shopify/DiscountCodeNode/1547497406770" })];
    if code_present {
        nodes.push(json!({ "id": DISCOUNT_FREE_SHIPPING_CODE_ID }));
    }
    if automatic_present {
        nodes.push(json!({ "id": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID }));
    }
    Value::Array(nodes)
}

pub(in crate::proxy) fn discount_free_shipping_code_node(phase: &str, status: &str) -> Value {
    json!({
        "id": DISCOUNT_FREE_SHIPPING_CODE_ID,
        "codeDiscount": discount_free_shipping_code_discount(phase, status)
    })
}

pub(in crate::proxy) fn discount_free_shipping_automatic_node(phase: &str, status: &str) -> Value {
    json!({
        "id": DISCOUNT_FREE_SHIPPING_AUTOMATIC_ID,
        "automaticDiscount": discount_free_shipping_automatic_discount(phase, status)
    })
}

pub(in crate::proxy) fn discount_free_shipping_code_discount(phase: &str, status: &str) -> Value {
    let created = phase == "create";
    json!({
        "__typename": "DiscountCodeFreeShipping",
        "title": if created { "HAR-196 code free shipping 1777150170404" } else { "HAR-196 code free shipping updated 1777150170404" },
        "status": status,
        "summary": if created { "Free shipping on one-time purchase products • Minimum purchase of $10.00 • For all countries • Applies to shipping rates under $25.00 • One use per customer" } else { "Free shipping on subscription products • Minimum purchase of $12.00 • For 2 countries • Applies to shipping rates under $30.00" },
        "startsAt": "2026-04-25T20:48:30Z",
        "endsAt": if status == "EXPIRED" { json!("2026-04-25T20:49:31Z") } else { Value::Null },
        "createdAt": "2026-04-25T20:49:30Z",
        "updatedAt": if created { "2026-04-25T20:49:30Z" } else { "2026-04-25T20:49:31Z" },
        "asyncUsageCount": 0,
        "discountClasses": ["SHIPPING"],
        "combinesWith": if created { json!({ "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }) } else { json!({ "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }) },
        "codes": {
            "nodes": [{
                "id": DISCOUNT_FREE_SHIPPING_REDEEM_ID,
                "code": if created { DISCOUNT_FREE_SHIPPING_INITIAL_CODE } else { DISCOUNT_FREE_SHIPPING_UPDATED_CODE },
                "asyncUsageCount": 0
            }],
            "pageInfo": { "hasNextPage": false, "hasPreviousPage": false, "startCursor": "eyJsYX...4In0=", "endCursor": "eyJsYX...4In0=" }
        },
        "context": { "__typename": "DiscountBuyerSelectionAll", "all": "ALL" },
        "minimumRequirement": { "__typename": "DiscountMinimumSubtotal", "greaterThanOrEqualToSubtotal": { "amount": if created { "10.0" } else { "12.0" }, "currencyCode": "CAD" } },
        "destinationSelection": if created { json!({ "__typename": "DiscountCountryAll", "allCountries": true }) } else { json!({ "__typename": "DiscountCountries", "countries": ["CA", "US"], "includeRestOfWorld": false }) },
        "maximumShippingPrice": { "amount": if created { "25.0" } else { "30.0" }, "currencyCode": "CAD" },
        "appliesOncePerCustomer": created,
        "appliesOnOneTimePurchase": created,
        "appliesOnSubscription": !created,
        "recurringCycleLimit": if created { 1 } else { 2 },
        "usageLimit": if created { 5 } else { 10 }
    })
}

pub(in crate::proxy) fn discount_free_shipping_automatic_discount(
    phase: &str,
    status: &str,
) -> Value {
    let created = phase == "create";
    json!({
        "__typename": "DiscountAutomaticFreeShipping",
        "title": if created { "HAR-196 automatic free shipping 1777150170404" } else { "HAR-196 automatic free shipping updated 1777150170404" },
        "status": status,
        "summary": if created { "Free shipping on all products • Minimum purchase of $15.00 • For all countries • Applies to shipping rates under $20.00" } else { "Free shipping on all products • Minimum purchase of $18.00 • For United States • Applies to shipping rates under $22.00" },
        "startsAt": "2026-04-25T20:48:30Z",
        "endsAt": if status == "EXPIRED" { json!("2026-04-25T20:49:31Z") } else { Value::Null },
        "createdAt": "2026-04-25T20:49:30Z",
        "updatedAt": if created { "2026-04-25T20:49:30Z" } else if status == "ACTIVE" { "2026-04-25T20:49:32Z" } else { "2026-04-25T20:49:31Z" },
        "asyncUsageCount": 0,
        "discountClasses": ["SHIPPING"],
        "combinesWith": if created { json!({ "productDiscounts": false, "orderDiscounts": true, "shippingDiscounts": false }) } else { json!({ "productDiscounts": true, "orderDiscounts": false, "shippingDiscounts": false }) },
        "context": { "__typename": "DiscountBuyerSelectionAll", "all": "ALL" },
        "minimumRequirement": { "__typename": "DiscountMinimumSubtotal", "greaterThanOrEqualToSubtotal": { "amount": if created { "15.0" } else { "18.0" }, "currencyCode": "CAD" } },
        "destinationSelection": if created { json!({ "__typename": "DiscountCountryAll", "allCountries": true }) } else { json!({ "__typename": "DiscountCountries", "countries": ["US"], "includeRestOfWorld": false }) },
        "maximumShippingPrice": { "amount": if created { "20.0" } else { "22.0" }, "currencyCode": "CAD" },
        "appliesOnOneTimePurchase": created,
        "appliesOnSubscription": !created,
        "recurringCycleLimit": if created { 1 } else { 3 }
    })
}

pub(in crate::proxy) fn discount_class_inference_mutation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.response_key.as_str() {
            "basicAll" => Some(discount_class_inference_payload(
                "DiscountCodeBasic",
                "HAR597CLASS1777950382203 basic order",
                &["ORDER"],
            )),
            "basicProduct" => Some(discount_class_inference_payload(
                "DiscountCodeBasic",
                "HAR597CLASS1777950382203 basic product",
                &["PRODUCT"],
            )),
            "basicCollection" => Some(discount_class_inference_payload(
                "DiscountCodeBasic",
                "HAR597CLASS1777950382203 basic collection",
                &["PRODUCT"],
            )),
            "bxgy" => Some(discount_class_inference_payload(
                "DiscountCodeBxgy",
                "HAR597CLASS1777950382203 bxgy product",
                &["PRODUCT"],
            )),
            "freeShipping" => Some(discount_class_inference_payload(
                "DiscountCodeFreeShipping",
                "HAR597CLASS1777950382203 free shipping",
                &["SHIPPING"],
            )),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_class_inference_payload(
    typename: &str,
    title: &str,
    classes: &[&str],
) -> Value {
    json!({
        "codeDiscountNode": {
            "codeDiscount": {
                "__typename": typename,
                "title": title,
                "discountClasses": classes
            }
        },
        "userErrors": []
    })
}

pub(in crate::proxy) fn discount_class_inference_read_data(fields: &[RootFieldSelection]) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        if field.name == "discountNodesCount" {
            let value = json!({ "count": 3, "precision": "EXACT" });
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) const DISCOUNT_CODE_BASIC_LIFECYCLE_ID: &str =
    "gid://shopify/DiscountCodeNode/1638844039474";
pub(in crate::proxy) const DISCOUNT_CODE_BASIC_LIFECYCLE_REDEEM_ID: &str =
    "gid://shopify/DiscountRedeemCode/21545225453874";
pub(in crate::proxy) const DISCOUNT_CODE_BASIC_LIFECYCLE_INITIAL_CODE: &str =
    "HAR193LIFE1777318334676";
pub(in crate::proxy) const DISCOUNT_CODE_BASIC_LIFECYCLE_UPDATED_CODE: &str =
    "HAR193LIVE1777318334676";

impl DraftProxy {
    pub(in crate::proxy) fn discount_code_basic_lifecycle_mutation_data(
        &mut self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            let value = match field.name.as_str() {
                "discountCodeBasicCreate" => {
                    self.store.staged.code_basic_lifecycle_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("create", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeBasicUpdate" => {
                    self.store.staged.code_basic_lifecycle_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDeactivate" => {
                    self.store.staged.code_basic_lifecycle_status = Some("EXPIRED".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("update", "EXPIRED"),
                        "userErrors": []
                    }))
                }
                "discountCodeActivate" => {
                    self.store.staged.code_basic_lifecycle_status = Some("ACTIVE".to_string());
                    Some(json!({
                        "codeDiscountNode": discount_code_basic_lifecycle_node("update", "ACTIVE"),
                        "userErrors": []
                    }))
                }
                "discountCodeDelete" => {
                    self.store.staged.code_basic_lifecycle_status = Some("DELETED".to_string());
                    Some(json!({
                        "deletedCodeDiscountId": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                        "userErrors": []
                    }))
                }
                _ => None,
            };
            if let Some(value) = value {
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn discount_code_basic_lifecycle_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        let status = self
            .store
            .staged
            .code_basic_lifecycle_status
            .as_deref()
            .unwrap_or("ACTIVE");
        let deleted = status == "DELETED";
        let active = status == "ACTIVE";
        for field in fields {
            let value = match field.name.as_str() {
                "discountNode" if deleted => Some(Value::Null),
                "discountNode" => Some(json!({
                    "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                    "discount": discount_code_basic_lifecycle_discount("update", status)
                })),
                "codeDiscountNodeByCode" if deleted => Some(Value::Null),
                "codeDiscountNodeByCode" => Some(json!({
                    "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                    "codeDiscount": discount_code_basic_lifecycle_discount("update", status)
                })),
                "discountNodes" => Some(json!({
                    "nodes": if active { json!([{ "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID }]) } else { json!([]) }
                })),
                "discountNodesCount" => Some(json!({
                    "count": if active { 1 } else { 0 },
                    "precision": "EXACT"
                })),
                _ => None,
            };
            if let Some(value) = value {
                let selected = if value.is_null() {
                    Value::Null
                } else {
                    selected_json(&value, &field.selection)
                };
                data.insert(field.response_key.clone(), selected);
            }
        }
        Value::Object(data)
    }

    pub(in crate::proxy) fn discount_code_basic_lifecycle_admin_node_read_data(
        &self,
        fields: &[RootFieldSelection],
    ) -> Value {
        let mut data = serde_json::Map::new();
        for field in fields {
            if field.name == "node" {
                let value = json!({
                    "__typename": "DiscountCodeNode",
                    "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
                    "codeDiscount": discount_code_basic_lifecycle_discount("update", "ACTIVE")
                });
                data.insert(
                    field.response_key.clone(),
                    selected_json(&value, &field.selection),
                );
            }
        }
        Value::Object(data)
    }
}

pub(in crate::proxy) fn discount_code_basic_lifecycle_node(phase: &str, status: &str) -> Value {
    json!({
        "id": DISCOUNT_CODE_BASIC_LIFECYCLE_ID,
        "codeDiscount": discount_code_basic_lifecycle_discount(phase, status)
    })
}

pub(in crate::proxy) fn discount_code_basic_lifecycle_discount(phase: &str, status: &str) -> Value {
    let created = phase == "create";
    json!({
        "__typename": "DiscountCodeBasic",
        "title": if created { "HAR-193 lifecycle 1777318334676" } else { "HAR-193 lifecycle updated 1777318334676" },
        "status": status,
        "summary": if created { "10% off one-time purchase products • Minimum purchase of $1.00" } else { "$5.00 off one-time purchase products • Minimum purchase of $2.00" },
        "startsAt": "2026-04-27T19:31:14Z",
        "endsAt": if status == "EXPIRED" { json!("2026-04-27T19:32:15Z") } else { Value::Null },
        "createdAt": "2026-04-27T19:32:14Z",
        "updatedAt": if created { "2026-04-27T19:32:14Z" } else { "2026-04-27T19:32:15Z" },
        "asyncUsageCount": 0,
        "discountClasses": ["ORDER"],
        "combinesWith": {
            "productDiscounts": false,
            "orderDiscounts": true,
            "shippingDiscounts": false
        },
        "codes": {
            "nodes": [{
                "id": DISCOUNT_CODE_BASIC_LIFECYCLE_REDEEM_ID,
                "code": if created { DISCOUNT_CODE_BASIC_LIFECYCLE_INITIAL_CODE } else { DISCOUNT_CODE_BASIC_LIFECYCLE_UPDATED_CODE },
                "asyncUsageCount": 0
            }],
            "pageInfo": {
                "hasNextPage": false,
                "hasPreviousPage": false,
                "startCursor": "eyJsYX...0In0=",
                "endCursor": "eyJsYX...0In0="
            }
        },
        "context": {
            "__typename": "DiscountBuyerSelectionAll",
            "all": "ALL"
        },
        "customerGets": {
            "value": if created { json!({
                "__typename": "DiscountPercentage",
                "percentage": 0.1
            }) } else { json!({
                "__typename": "DiscountAmount",
                "amount": { "amount": "5.0", "currencyCode": "CAD" },
                "appliesOnEachItem": false
            }) },
            "items": {
                "__typename": "AllDiscountItems",
                "allItems": true
            },
            "appliesOnOneTimePurchase": true,
            "appliesOnSubscription": false
        },
        "minimumRequirement": {
            "__typename": "DiscountMinimumSubtotal",
            "greaterThanOrEqualToSubtotal": {
                "amount": if created { "1.0" } else { "2.0" },
                "currencyCode": "CAD"
            }
        }
    })
}

pub(in crate::proxy) const DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID: &str =
    "gid://shopify/DiscountCodeNode/1638894633266";
pub(in crate::proxy) const DISCOUNT_BUYER_CONTEXT_CUSTOMER_ID: &str =
    "gid://shopify/Customer/10548596015410";
pub(in crate::proxy) const DISCOUNT_BUYER_CONTEXT_SEGMENT_ID: &str =
    "gid://shopify/Segment/647746715954";

pub(in crate::proxy) fn discount_code_basic_buyer_context_mutation_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountCodeBasicCreate" => Some(json!({
                "codeDiscountNode": discount_code_basic_buyer_context_node("customer"),
                "userErrors": []
            })),
            "discountCodeBasicUpdate" => Some(json!({
                "codeDiscountNode": discount_code_basic_buyer_context_node("segment"),
                "userErrors": []
            })),
            "discountCodeDelete" => Some(json!({
                "deletedCodeDiscountId": DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID,
                "userErrors": []
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_code_basic_buyer_context_read_data(
    fields: &[RootFieldSelection],
) -> Value {
    let mut data = serde_json::Map::new();
    for field in fields {
        let value = match field.name.as_str() {
            "discountNode" => Some(json!({
                "id": DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID,
                "discount": discount_code_basic_buyer_context_discount("segment")
            })),
            "codeDiscountNodeByCode" => Some(json!({
                "codeDiscount": discount_code_basic_buyer_context_discount("segment")
            })),
            _ => None,
        };
        if let Some(value) = value {
            data.insert(
                field.response_key.clone(),
                selected_json(&value, &field.selection),
            );
        }
    }
    Value::Object(data)
}

pub(in crate::proxy) fn discount_code_basic_buyer_context_node(context: &str) -> Value {
    json!({
        "id": DISCOUNT_CODE_BASIC_BUYER_CONTEXT_ID,
        "codeDiscount": discount_code_basic_buyer_context_discount(context)
    })
}

pub(in crate::proxy) fn discount_code_basic_buyer_context_discount(context: &str) -> Value {
    let (title, code, context_value) = if context == "customer" {
        (
            "HAR-390 code customer context 1777346878525",
            "HAR390CTX1777346878525",
            json!({
                "__typename": "DiscountCustomers",
                "customers": [{
                    "__typename": "Customer",
                    "id": DISCOUNT_BUYER_CONTEXT_CUSTOMER_ID,
                    "displayName": "HAR390 Buyer Context"
                }]
            }),
        )
    } else {
        (
            "HAR-390 code segment context 1777346878525",
            "HAR390SEG1777346878525",
            json!({
                "__typename": "DiscountCustomerSegments",
                "segments": [{
                    "__typename": "Segment",
                    "id": DISCOUNT_BUYER_CONTEXT_SEGMENT_ID,
                    "name": "HAR-390 buyer context 1777346878525"
                }]
            }),
        )
    };
    json!({
        "__typename": "DiscountCodeBasic",
        "title": title,
        "status": "ACTIVE",
        "codes": {
            "nodes": [{
                "code": code,
                "asyncUsageCount": 0
            }]
        },
        "context": context_value
    })
}

pub(in crate::proxy) fn discount_automatic_basic_buyer_context_mutation(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let payload = match root_field {
        "discountAutomaticBasicCreate" => json!({
            "automaticDiscountNode": discount_automatic_basic_buyer_context_node("customer"),
            "userErrors": []
        }),
        "discountAutomaticBasicUpdate" => {
            let id = resolved_string_arg(variables, "id")?;
            if id != DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID {
                return None;
            }
            json!({
                "automaticDiscountNode": discount_automatic_basic_buyer_context_node("segment"),
                "userErrors": []
            })
        }
        "discountAutomaticDelete" => {
            let id = resolved_string_arg(variables, "id")?;
            if id != DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID {
                return None;
            }
            json!({
                "deletedAutomaticDiscountId": DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID,
                "userErrors": []
            })
        }
        _ => return None,
    };
    let payload_selection = root_field_selection(query).unwrap_or_default();
    Some(ok_json(json!({
        "data": {
            root_field: selected_json(&payload, &payload_selection)
        }
    })))
}

pub(in crate::proxy) fn discount_automatic_basic_buyer_context_read(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    let id = resolved_string_arg(variables, "id")?;
    if id != DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID {
        return None;
    }
    let node = discount_automatic_basic_buyer_context_node("segment");
    let selection = root_field_selection(query).unwrap_or_default();
    Some(ok_json(json!({
        "data": {
            "automaticDiscountNode": selected_json(&node, &selection)
        }
    })))
}

pub(in crate::proxy) const DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID: &str =
    "gid://shopify/DiscountAutomaticNode/1638894666034";

pub(in crate::proxy) fn discount_automatic_basic_buyer_context_node(context: &str) -> Value {
    let (title, context_value) = if context == "customer" {
        (
            "HAR-390 automatic customer context 1777346878525",
            json!({
                "__typename": "DiscountCustomers",
                "customers": [{
                    "__typename": "Customer",
                    "id": "gid://shopify/Customer/10548596015410",
                    "displayName": "HAR390 Buyer Context"
                }]
            }),
        )
    } else {
        (
            "HAR-390 automatic segment context 1777346878525",
            json!({
                "__typename": "DiscountCustomerSegments",
                "segments": [{
                    "__typename": "Segment",
                    "id": "gid://shopify/Segment/647746715954",
                    "name": "HAR-390 buyer context 1777346878525"
                }]
            }),
        )
    };
    json!({
        "id": DISCOUNT_AUTOMATIC_BASIC_BUYER_CONTEXT_ID,
        "automaticDiscount": {
            "__typename": "DiscountAutomaticBasic",
            "title": title,
            "status": "ACTIVE",
            "context": context_value
        }
    })
}

pub(in crate::proxy) fn discount_activate_deactivate_noop_response(
    root_field: &str,
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Response> {
    if !query.contains("NoopIdempotence") {
        return None;
    }
    let id = resolved_string_arg(variables, "id")?;
    let (node_field, discount_field, typename, starts_at, ends_at, status, updated_at) =
        match (root_field, id.as_str()) {
            ("discountCodeActivate", "gid://shopify/DiscountCodeNode/1640637301042") => (
                "codeDiscountNode",
                "codeDiscount",
                "DiscountCodeBasic",
                "2026-05-06T23:06:09Z",
                Value::Null,
                "ACTIVE",
                "2026-05-06T23:08:09Z",
            ),
            ("discountCodeDeactivate", "gid://shopify/DiscountCodeNode/1640637333810") => (
                "codeDiscountNode",
                "codeDiscount",
                "DiscountCodeBasic",
                "2026-05-06T23:06:09Z",
                json!("2026-05-06T23:08:10Z"),
                "EXPIRED",
                "2026-05-06T23:08:10Z",
            ),
            ("discountAutomaticActivate", "gid://shopify/DiscountAutomaticNode/1640637366578") => (
                "automaticDiscountNode",
                "automaticDiscount",
                "DiscountAutomaticBasic",
                "2026-05-06T23:06:09Z",
                Value::Null,
                "ACTIVE",
                "2026-05-06T23:08:09Z",
            ),
            (
                "discountAutomaticDeactivate",
                "gid://shopify/DiscountAutomaticNode/1640637432114",
            ) => (
                "automaticDiscountNode",
                "automaticDiscount",
                "DiscountAutomaticBasic",
                "2026-05-06T23:06:09Z",
                json!("2026-05-06T23:08:10Z"),
                "EXPIRED",
                "2026-05-06T23:08:10Z",
            ),
            _ => return None,
        };

    let payload = json!({
        node_field: {
            "id": id,
            discount_field: {
                "__typename": typename,
                "startsAt": starts_at,
                "endsAt": ends_at,
                "status": status,
                "updatedAt": updated_at,
            }
        },
        "userErrors": []
    });
    let payload_selection = root_field_selection(query).unwrap_or_default();
    Some(ok_json(json!({
        "data": {
            root_field: selected_json(&payload, &payload_selection)
        }
    })))
}
