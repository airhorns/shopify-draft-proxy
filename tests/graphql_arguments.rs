use std::collections::BTreeMap;

use pretty_assertions::assert_eq;
use shopify_draft_proxy::graphql::{root_field_arguments, ResolvedValue};

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
fn root_field_arguments_resolve_variables_and_missing_variables_as_null() {
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
}
