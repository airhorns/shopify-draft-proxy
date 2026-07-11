use std::collections::BTreeMap;

use pretty_assertions::assert_eq;
use shopify_draft_proxy::graphql::{
    parse_operation_with_variables, parsed_document, root_field_arguments, root_fields,
    selected_operation_query, variables_with_operation_defaults, RawArgumentValue, ResolvedValue,
};

#[test]
fn root_field_arguments_resolve_literals_and_enums() {
    let args = root_field_arguments(
        r#"{ products(first: 10, query: "foo", active: true, ratio: 1.5, sort: ASCENDING, tags: ["a", "b"], filter: { id: "1", limit: null }) { id } }"#,
        &BTreeMap::new(),
    )
    .expect("arguments should resolve");

    assert_eq!(args.get("first"), Some(&ResolvedValue::Int(10)));
    assert_eq!(
        args.get("query"),
        Some(&ResolvedValue::String("foo".to_string()))
    );
    assert_eq!(args.get("active"), Some(&ResolvedValue::Bool(true)));
    assert_eq!(args.get("ratio"), Some(&ResolvedValue::Float(1.5)));
    assert_eq!(
        args.get("sort"),
        Some(&ResolvedValue::String("ASCENDING".to_string()))
    );
    assert_eq!(
        args.get("tags"),
        Some(&ResolvedValue::List(vec![
            ResolvedValue::String("a".to_string()),
            ResolvedValue::String("b".to_string())
        ]))
    );

    let filter = BTreeMap::from([
        ("id".to_string(), ResolvedValue::String("1".to_string())),
        ("limit".to_string(), ResolvedValue::Null),
    ]);
    assert_eq!(args.get("filter"), Some(&ResolvedValue::Object(filter)));
}

#[test]
fn root_field_arguments_keep_resolved_compatibility_while_raw_arguments_track_unbound_variables() {
    let variables = BTreeMap::from([
        ("first".to_string(), ResolvedValue::Int(25)),
        (
            "after".to_string(),
            ResolvedValue::String("cursor-1".to_string()),
        ),
    ]);

    let args = root_field_arguments(
        "query Q($first: Int!, $after: String, $missing: String) { products(first: $first, after: $after, missing: $missing) { id } }",
        &variables,
    )
    .expect("arguments should resolve");

    assert_eq!(args.get("first"), Some(&ResolvedValue::Int(25)));
    assert_eq!(
        args.get("after"),
        Some(&ResolvedValue::String("cursor-1".to_string()))
    );
    assert_eq!(args.get("missing"), Some(&ResolvedValue::Null));

    let fields = root_fields(
        "query Q($first: Int!, $after: String, $missing: String) { products(first: $first, after: $after, missing: $missing) { id } }",
        &variables,
    )
    .expect("root fields should parse");
    let products = fields.first().expect("products root field");
    assert_eq!(
        products.raw_arguments.get("missing"),
        Some(&RawArgumentValue::Variable {
            name: "missing".to_string(),
            value: None
        })
    );
    assert_eq!(
        products.raw_arguments.get("after"),
        Some(&RawArgumentValue::Variable {
            name: "after".to_string(),
            value: Some(ResolvedValue::String("cursor-1".to_string()))
        })
    );
}

#[test]
fn parsed_document_preserves_operation_metadata_aliases_fragments_and_locations() {
    let document = parsed_document(
        r#"
        fragment ProductFields on Product {
          titleAlias: title
        }

        query ProductLookup {
          productAlias: product(id: "gid://shopify/Product/1") {
            id
            ...ProductFields
            ... on Product {
              handleAlias: handle
            }
          }
        }
        "#,
        &BTreeMap::new(),
    )
    .expect("document should parse");

    assert_eq!(document.operation_name.as_deref(), Some("ProductLookup"));
    assert_eq!(document.operation_path, "query ProductLookup");
    assert_eq!(document.location.line, 6);
    assert_eq!(document.root_fields.len(), 1);

    let product = &document.root_fields[0];
    assert_eq!(product.name, "product");
    assert_eq!(product.response_key, "productAlias");
    assert_eq!(product.location.line, 7);
    assert_eq!(
        product.raw_arguments.get("id"),
        Some(&RawArgumentValue::String(
            "gid://shopify/Product/1".to_string()
        ))
    );
    assert_eq!(product.selection[0].name, "id");
    assert_eq!(product.selection[1].name, "title");
    assert_eq!(product.selection[1].response_key, "titleAlias");
    assert_eq!(product.selection[2].name, "handle");
    assert_eq!(product.selection[2].response_key, "handleAlias");
}

#[test]
fn parsed_document_applies_conditional_directives_to_roots_fields_and_fragments() {
    let variables = BTreeMap::from([
        ("includeProduct".to_string(), ResolvedValue::Bool(true)),
        ("includeTitle".to_string(), ResolvedValue::Bool(true)),
        ("includeInline".to_string(), ResolvedValue::Bool(false)),
        ("includeSpread".to_string(), ResolvedValue::Bool(true)),
    ]);
    let document = parsed_document(
        r#"
        fragment ProductFields on Product {
          aliasTitle: title @include(if: $includeTitle)
          skippedVendor: vendor @skip(if: true)
        }

        query ConditionalDirectives(
          $includeProduct: Boolean!,
          $includeTitle: Boolean!,
          $includeInline: Boolean!,
          $includeSpread: Boolean!
        ) {
          skippedProducts: products(first: 1) @skip(if: true) {
            nodes { id }
          }
          skippedProduct: product(id: "gid://shopify/Product/2") @include(if: false) {
            id
          }
          product(id: "gid://shopify/Product/1") @include(if: $includeProduct) {
            id
            hiddenHandle: handle @include(if: false)
            ... on Product @include(if: $includeInline) {
              seo { title }
            }
            ...ProductFields @include(if: $includeSpread)
          }
        }
        "#,
        &variables,
    )
    .expect("document should parse");

    assert_eq!(document.root_fields.len(), 1);
    let product = &document.root_fields[0];
    assert_eq!(product.name, "product");
    assert_eq!(
        product
            .selection
            .iter()
            .map(|field| field.response_key.as_str())
            .collect::<Vec<_>>(),
        vec!["id", "aliasTitle"]
    );
}

#[test]
fn parsed_document_applies_conditional_directives_from_parity_documents() {
    let read_variables = BTreeMap::from([
        (
            "id".to_string(),
            ResolvedValue::String("gid://shopify/Product/1".to_string()),
        ),
        ("includeTitle".to_string(), ResolvedValue::Bool(false)),
        ("skipInline".to_string(), ResolvedValue::Bool(true)),
        ("includeSpread".to_string(), ResolvedValue::Bool(true)),
    ]);
    let document = parsed_document(
        include_str!("../config/parity-requests/products/graphql-conditional-directives-product-read.graphql"),
        &read_variables,
    )
    .expect("document should parse");

    assert_eq!(document.root_fields.len(), 1);
    let product = &document.root_fields[0];
    assert_eq!(product.response_key, "includedProduct");
    assert_eq!(
        product
            .selection
            .iter()
            .map(|field| field.response_key.as_str())
            .collect::<Vec<_>>(),
        vec!["id", "aliasStatus"]
    );

    let skipped_mutation_variables = BTreeMap::from([
        (
            "product".to_string(),
            ResolvedValue::Object(BTreeMap::from([(
                "title".to_string(),
                ResolvedValue::String("Skipped".to_string()),
            )])),
        ),
        ("skipMutation".to_string(), ResolvedValue::Bool(true)),
    ]);
    let operation = parse_operation_with_variables(
        include_str!("../config/parity-requests/products/graphql-conditional-directives-product-create-skipped.graphql"),
        &skipped_mutation_variables,
    )
    .expect("mutation should parse");
    assert!(operation.root_fields.is_empty());
}

#[test]
fn root_fields_preserve_omitted_null_and_unbound_nested_arguments() {
    let fields = root_fields(
        r#"
        mutation DeleteProduct($id: ID) {
          productDelete(input: { id: $id, reason: null }) {
            deletedProductId
          }
        }
        "#,
        &BTreeMap::new(),
    )
    .expect("root fields should parse");

    let product_delete = fields.first().expect("productDelete root field");
    let RawArgumentValue::Object(input) = product_delete
        .raw_arguments
        .get("input")
        .expect("input arg should be present")
    else {
        panic!("input should be an object literal");
    };

    assert!(!input.contains_key("omitted"));
    assert_eq!(input.get("reason"), Some(&RawArgumentValue::Null));
    assert_eq!(
        input.get("id"),
        Some(&RawArgumentValue::Variable {
            name: "id".to_string(),
            value: None
        })
    );
}

#[test]
fn selected_operation_query_filters_non_selected_operations_and_keeps_fragments() {
    let query = r#"
        query First($id: ID!) {
          product(id: $id) { ...ProductFields }
        }

        fragment ProductFields on Product {
          id
        }

        query Second($first: Int = 1) {
          products(first: $first) { nodes { id } }
        }
    "#;

    let selected =
        selected_operation_query(query, Some("Second")).expect("operation should be selected");

    assert!(selected.contains("query Second"));
    assert!(selected.contains("fragment ProductFields"));
    assert!(!selected.contains("query First"));
    assert!(!selected.contains("product(id: $id)"));
}

#[test]
fn selected_operation_variable_defaults_merge_only_omitted_values() {
    let query = r#"
        query Defaults($first: Int = 1, $query: String = "snow", $explicit: String = "default") {
          products(first: $first, query: $query) { nodes { id } }
          shop { name }
        }
    "#;
    let variables = BTreeMap::from([("explicit".to_string(), ResolvedValue::Null)]);

    let resolved =
        variables_with_operation_defaults(query, &variables, None).expect("defaults should merge");

    assert_eq!(resolved.get("first"), Some(&ResolvedValue::Int(1)));
    assert_eq!(
        resolved.get("query"),
        Some(&ResolvedValue::String("snow".to_string()))
    );
    assert_eq!(resolved.get("explicit"), Some(&ResolvedValue::Null));
}
