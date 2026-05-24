use super::*;

pub(in crate::proxy) fn resolved_value_json(value: &ResolvedValue) -> Value {
    match value {
        ResolvedValue::String(value) => json!(value),
        ResolvedValue::Int(value) => json!(value),
        ResolvedValue::Float(value) => json!(value),
        ResolvedValue::Bool(value) => json!(value),
        ResolvedValue::Null => Value::Null,
        ResolvedValue::List(values) => {
            Value::Array(values.iter().map(resolved_value_json).collect())
        }
        ResolvedValue::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(name, value)| (name.clone(), resolved_value_json(value)))
                .collect(),
        ),
    }
}

pub(in crate::proxy) fn resolved_variables_json(
    variables: &BTreeMap<String, ResolvedValue>,
) -> Value {
    Value::Object(
        variables
            .iter()
            .map(|(name, value)| (name.clone(), resolved_value_json(value)))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_nested_resolved_values_to_json() {
        let value = ResolvedValue::Object(BTreeMap::from([
            (
                "items".to_string(),
                ResolvedValue::List(vec![
                    ResolvedValue::String("gid://shopify/Product/1".to_string()),
                    ResolvedValue::Null,
                ]),
            ),
            ("enabled".to_string(), ResolvedValue::Bool(true)),
            ("count".to_string(), ResolvedValue::Int(2)),
            ("ratio".to_string(), ResolvedValue::Float(2.5)),
        ]));

        assert_eq!(
            resolved_value_json(&value),
            json!({
                "items": ["gid://shopify/Product/1", null],
                "enabled": true,
                "count": 2,
                "ratio": 2.5
            })
        );
    }

    #[test]
    fn serializes_resolved_variable_maps() {
        let variables = BTreeMap::from([
            (
                "id".to_string(),
                ResolvedValue::String("gid://shopify/App/1".to_string()),
            ),
            ("dryRun".to_string(), ResolvedValue::Bool(false)),
        ]);

        assert_eq!(
            resolved_variables_json(&variables),
            json!({
                "dryRun": false,
                "id": "gid://shopify/App/1"
            })
        );
    }
}
