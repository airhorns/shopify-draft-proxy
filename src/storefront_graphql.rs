//! Executable Shopify Storefront GraphQL schemas.
//!
//! Storefront captures are kept independently from Admin captures because the
//! two APIs intentionally reuse root and object names with different types and
//! semantics. The Storefront capture is authenticated introspection JSON; this
//! module renders that immutable type graph to SDL once and passes it through
//! the same dynamic-schema builder used by Admin.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    sync::OnceLock,
};

use async_graphql::dynamic::Schema;
use graphql_parser::{
    query::{
        parse_query, Definition, FragmentDefinition, InlineFragment, OperationDefinition,
        Selection, SelectionSet, TypeCondition,
    },
    Style,
};
use serde_json::Value;

use crate::{
    admin_graphql::{build_schema_from_sdl, SchemaBuildError},
    graphql::OperationType,
};

pub use crate::graphql_catalog::StorefrontApiVersion;

static SCHEMAS: [OnceLock<Schema>; StorefrontApiVersion::COUNT] =
    [const { OnceLock::new() }; StorefrontApiVersion::COUNT];
static SDLS: [OnceLock<Result<String, String>>; StorefrontApiVersion::COUNT] =
    [const { OnceLock::new() }; StorefrontApiVersion::COUNT];

pub fn schema(version: StorefrontApiVersion) -> Result<&'static Schema, SchemaBuildError> {
    let slot = &SCHEMAS[version.index()];
    if let Some(schema) = slot.get() {
        return Ok(schema);
    }
    let built = build_schema_from_sdl(
        schema_sdl(version)?,
        crate::operation_registry::ApiSurface::Storefront,
        version.as_str(),
    )?;
    let _ = slot.set(built);
    Ok(slot
        .get()
        .expect("versioned Storefront GraphQL schema should be initialized"))
}

pub fn root_field_named_type(
    version: StorefrontApiVersion,
    operation_type: OperationType,
    field_name: &str,
) -> Option<String> {
    let schema = schema(version).ok()?;
    let root_name = match operation_type {
        OperationType::Query => Some(schema.registry().query_type.as_str()),
        OperationType::Mutation => schema.registry().mutation_type.as_deref(),
        OperationType::Subscription => schema.registry().subscription_type.as_deref(),
    }?;
    let field = schema
        .registry()
        .types
        .get(root_name)?
        .field_by_name(field_name)?;
    named_type_from_display(&field.ty)
}

pub(crate) fn root_field_names(
    version: StorefrontApiVersion,
    operation_type: OperationType,
) -> Vec<String> {
    let Ok(schema) = schema(version) else {
        return Vec::new();
    };
    let root_name = match operation_type {
        OperationType::Query => Some(schema.registry().query_type.as_str()),
        OperationType::Mutation => schema.registry().mutation_type.as_deref(),
        OperationType::Subscription => schema.registry().subscription_type.as_deref(),
    };
    root_name
        .and_then(|root_name| schema.registry().types.get(root_name))
        .and_then(async_graphql::registry::MetaType::fields)
        .map(|fields| fields.keys().cloned().collect())
        .unwrap_or_default()
}

/// Work around async-graphql dynamic-schema execution dropping named fragment
/// spreads whose type condition is a union. Validation understands these
/// spreads, but the dynamic executor only recognizes concrete types and
/// implemented interfaces while collecting fields. Inline the selection set
/// for valid union spreads so the engine can still own concrete fragment
/// selection, directives, aliases, and output projection.
pub(crate) fn expand_dynamic_union_fragment_spreads(schema: &Schema, query: &str) -> String {
    let Ok(mut document) = parse_query::<String>(query) else {
        return query.to_string();
    };
    let fragments = document
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Fragment(fragment) => Some((fragment.name.clone(), fragment.clone())),
            Definition::Operation(_) => None,
        })
        .collect::<BTreeMap<_, _>>();
    let union_fragments = fragments
        .iter()
        .filter_map(|(name, fragment)| {
            let type_name = type_condition_name(&fragment.type_condition);
            matches!(
                schema.registry().types.get(type_name),
                Some(async_graphql::registry::MetaType::Union { .. })
            )
            .then(|| name.clone())
        })
        .collect::<BTreeSet<_>>();
    if union_fragments.is_empty() {
        return query.to_string();
    }

    let mut expanded_fragments = BTreeSet::new();
    let mut active_fragments = BTreeSet::new();
    for definition in &mut document.definitions {
        let (parent_type, selection_set) = match definition {
            Definition::Operation(OperationDefinition::SelectionSet(selection_set)) => {
                (schema.registry().query_type.clone(), selection_set)
            }
            Definition::Operation(OperationDefinition::Query(query)) => (
                schema.registry().query_type.clone(),
                &mut query.selection_set,
            ),
            Definition::Operation(OperationDefinition::Mutation(mutation)) => {
                let Some(parent_type) = schema.registry().mutation_type.clone() else {
                    continue;
                };
                (parent_type, &mut mutation.selection_set)
            }
            Definition::Operation(OperationDefinition::Subscription(subscription)) => {
                let Some(parent_type) = schema.registry().subscription_type.clone() else {
                    continue;
                };
                (parent_type, &mut subscription.selection_set)
            }
            Definition::Fragment(fragment) => {
                if union_fragments.contains(&fragment.name) {
                    continue;
                }
                (
                    type_condition_name(&fragment.type_condition).to_string(),
                    &mut fragment.selection_set,
                )
            }
        };
        if expand_union_spreads_in_selection_set(
            schema,
            selection_set,
            &parent_type,
            &fragments,
            &union_fragments,
            &mut active_fragments,
            &mut expanded_fragments,
        )
        .is_err()
        {
            // Preserve the original invalid document so the GraphQL engine can
            // report fragment-cycle validation errors normally.
            return query.to_string();
        }
    }
    document.definitions.retain(|definition| {
        !matches!(definition, Definition::Fragment(fragment) if expanded_fragments.contains(&fragment.name))
    });
    document.format(&Style::default())
}

#[allow(clippy::too_many_arguments)]
fn expand_union_spreads_in_selection_set<'a>(
    schema: &Schema,
    selection_set: &mut SelectionSet<'a, String>,
    parent_type: &str,
    fragments: &BTreeMap<String, FragmentDefinition<'a, String>>,
    union_fragments: &BTreeSet<String>,
    active_fragments: &mut BTreeSet<String>,
    expanded_fragments: &mut BTreeSet<String>,
) -> Result<(), ()> {
    let mut expanded = Vec::new();
    for selection in std::mem::take(&mut selection_set.items) {
        match selection {
            Selection::Field(mut field) => {
                if let Some(child_type) = schema
                    .registry()
                    .types
                    .get(parent_type)
                    .and_then(|parent| parent.field_by_name(&field.name))
                    .and_then(|field| named_type_from_display(&field.ty))
                {
                    expand_union_spreads_in_selection_set(
                        schema,
                        &mut field.selection_set,
                        &child_type,
                        fragments,
                        union_fragments,
                        active_fragments,
                        expanded_fragments,
                    )?;
                }
                expanded.push(Selection::Field(field));
            }
            Selection::InlineFragment(mut fragment) => {
                let fragment_parent = fragment
                    .type_condition
                    .as_ref()
                    .map(type_condition_name)
                    .unwrap_or(parent_type)
                    .to_string();
                expand_union_spreads_in_selection_set(
                    schema,
                    &mut fragment.selection_set,
                    &fragment_parent,
                    fragments,
                    union_fragments,
                    active_fragments,
                    expanded_fragments,
                )?;
                expanded.push(Selection::InlineFragment(fragment));
            }
            Selection::FragmentSpread(spread)
                if union_fragments.contains(&spread.fragment_name) =>
            {
                let Some(fragment) = fragments.get(&spread.fragment_name) else {
                    expanded.push(Selection::FragmentSpread(spread));
                    continue;
                };
                let union_type = type_condition_name(&fragment.type_condition);
                if !output_types_overlap(schema, parent_type, union_type) {
                    // Leave impossible spreads intact for normal GraphQL
                    // validation instead of broadening their applicability.
                    expanded.push(Selection::FragmentSpread(spread));
                    continue;
                }
                if !active_fragments.insert(spread.fragment_name.clone()) {
                    return Err(());
                }
                let mut fragment_selection = fragment.selection_set.clone();
                expand_union_spreads_in_selection_set(
                    schema,
                    &mut fragment_selection,
                    parent_type,
                    fragments,
                    union_fragments,
                    active_fragments,
                    expanded_fragments,
                )?;
                active_fragments.remove(&spread.fragment_name);
                expanded_fragments.insert(spread.fragment_name.clone());

                let mut replacement = Selection::InlineFragment(InlineFragment {
                    position: fragment.position,
                    type_condition: None,
                    directives: fragment.directives.clone(),
                    selection_set: fragment_selection,
                });
                if !spread.directives.is_empty() {
                    replacement = Selection::InlineFragment(InlineFragment {
                        position: spread.position,
                        type_condition: None,
                        directives: spread.directives,
                        selection_set: SelectionSet {
                            span: selection_set.span,
                            items: vec![replacement],
                        },
                    });
                }
                expanded.push(replacement);
            }
            selection => expanded.push(selection),
        }
    }
    selection_set.items = expanded;
    Ok(())
}

fn type_condition_name<'document, 'borrow>(
    type_condition: &'borrow TypeCondition<'document, String>,
) -> &'borrow str {
    match type_condition {
        TypeCondition::On(name) => name,
    }
}

fn output_types_overlap(schema: &Schema, left: &str, right: &str) -> bool {
    let concrete_types = |name: &str| match schema.registry().types.get(name) {
        Some(async_graphql::registry::MetaType::Object { .. }) => {
            BTreeSet::from([name.to_string()])
        }
        Some(
            async_graphql::registry::MetaType::Interface { possible_types, .. }
            | async_graphql::registry::MetaType::Union { possible_types, .. },
        ) => possible_types.iter().cloned().collect(),
        _ => BTreeSet::new(),
    };
    let left = concrete_types(left);
    let right = concrete_types(right);
    left.iter().any(|name| right.contains(name))
}

fn schema_sdl(version: StorefrontApiVersion) -> Result<&'static str, SchemaBuildError> {
    let slot = &SDLS[version.index()];
    match slot.get_or_init(|| {
        let capture: Value = serde_json::from_str(version.introspection_capture())
            .map_err(|error| format!("invalid Storefront introspection JSON: {error}"))?;
        render_introspection_sdl(&capture)
    }) {
        Ok(sdl) => Ok(sdl),
        Err(error) => Err(SchemaBuildError::Parse(error.clone())),
    }
}

fn render_introspection_sdl(capture: &Value) -> Result<String, String> {
    let schema = capture
        .get("schema")
        .and_then(Value::as_object)
        .ok_or_else(|| "Storefront capture has no schema object".to_string())?;
    let query_root = capture
        .pointer("/schema/queryType/name")
        .and_then(Value::as_str)
        .ok_or_else(|| "Storefront capture has no query root".to_string())?;
    let mutation_root = capture
        .pointer("/schema/mutationType/name")
        .and_then(Value::as_str);
    let subscription_root = capture
        .pointer("/schema/subscriptionType/name")
        .and_then(Value::as_str);
    let types = schema
        .get("types")
        .and_then(Value::as_array)
        .ok_or_else(|| "Storefront capture has no types array".to_string())?;

    let mut output = String::new();
    writeln!(output, "schema {{").expect("writing to String cannot fail");
    writeln!(output, "  query: {query_root}").expect("writing to String cannot fail");
    if let Some(mutation_root) = mutation_root {
        writeln!(output, "  mutation: {mutation_root}").expect("writing to String cannot fail");
    }
    if let Some(subscription_root) = subscription_root {
        writeln!(output, "  subscription: {subscription_root}")
            .expect("writing to String cannot fail");
    }
    writeln!(output, "}}\n").expect("writing to String cannot fail");

    for schema_type in types {
        let Some(name) = schema_type.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name.starts_with("__") {
            continue;
        }
        match schema_type.get("kind").and_then(Value::as_str) {
            Some("SCALAR") => {
                writeln!(output, "scalar {name}\n").expect("writing to String cannot fail");
            }
            Some("OBJECT") => render_composite_type(&mut output, "type", schema_type, name)?,
            Some("INTERFACE") => {
                render_composite_type(&mut output, "interface", schema_type, name)?
            }
            Some("UNION") => render_union(&mut output, schema_type, name)?,
            Some("ENUM") => render_enum(&mut output, schema_type, name)?,
            Some("INPUT_OBJECT") => render_input_object(&mut output, schema_type, name)?,
            Some(kind) => return Err(format!("unsupported Storefront introspection kind {kind}")),
            None => return Err(format!("Storefront type {name} has no kind")),
        }
    }
    Ok(output)
}

fn render_composite_type(
    output: &mut String,
    keyword: &str,
    schema_type: &Value,
    name: &str,
) -> Result<(), String> {
    write!(output, "{keyword} {name}").expect("writing to String cannot fail");
    let interfaces = schema_type
        .get("interfaces")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|interface| interface.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if !interfaces.is_empty() {
        write!(output, " implements {}", interfaces.join(" & "))
            .expect("writing to String cannot fail");
    }
    writeln!(output, " {{").expect("writing to String cannot fail");
    for field in schema_type
        .get("fields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let field_name = required_name(field, "field")?;
        write!(output, "  {field_name}").expect("writing to String cannot fail");
        render_arguments(output, field)?;
        let field_type = type_ref_sdl(
            field
                .get("type")
                .ok_or_else(|| format!("Storefront field {name}.{field_name} has no type"))?,
        )?;
        write!(output, ": {field_type}").expect("writing to String cannot fail");
        render_deprecation(output, field);
        writeln!(output).expect("writing to String cannot fail");
    }
    writeln!(output, "}}\n").expect("writing to String cannot fail");
    Ok(())
}

fn render_arguments(output: &mut String, field: &Value) -> Result<(), String> {
    let arguments = field
        .get("args")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    if arguments.is_empty() {
        return Ok(());
    }
    write!(output, "(").expect("writing to String cannot fail");
    for (index, argument) in arguments.iter().enumerate() {
        if index > 0 {
            write!(output, ", ").expect("writing to String cannot fail");
        }
        let name = required_name(argument, "argument")?;
        let argument_type = type_ref_sdl(
            argument
                .get("type")
                .ok_or_else(|| format!("Storefront argument {name} has no type"))?,
        )?;
        write!(output, "{name}: {argument_type}").expect("writing to String cannot fail");
        render_default_value(output, argument);
        render_deprecation(output, argument);
    }
    write!(output, ")").expect("writing to String cannot fail");
    Ok(())
}

fn render_union(output: &mut String, schema_type: &Value, name: &str) -> Result<(), String> {
    let possible_types = schema_type
        .get("possibleTypes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|possible_type| possible_type.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    if possible_types.is_empty() {
        return Err(format!("Storefront union {name} has no possible types"));
    }
    writeln!(output, "union {name} = {}\n", possible_types.join(" | "))
        .expect("writing to String cannot fail");
    Ok(())
}

fn render_enum(output: &mut String, schema_type: &Value, name: &str) -> Result<(), String> {
    writeln!(output, "enum {name} {{").expect("writing to String cannot fail");
    for value in schema_type
        .get("enumValues")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let value_name = required_name(value, "enum value")?;
        write!(output, "  {value_name}").expect("writing to String cannot fail");
        render_deprecation(output, value);
        writeln!(output).expect("writing to String cannot fail");
    }
    writeln!(output, "}}\n").expect("writing to String cannot fail");
    Ok(())
}

fn render_input_object(output: &mut String, schema_type: &Value, name: &str) -> Result<(), String> {
    writeln!(output, "input {name} {{").expect("writing to String cannot fail");
    for field in schema_type
        .get("inputFields")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let field_name = required_name(field, "input field")?;
        let field_type =
            type_ref_sdl(field.get("type").ok_or_else(|| {
                format!("Storefront input field {name}.{field_name} has no type")
            })?)?;
        write!(output, "  {field_name}: {field_type}").expect("writing to String cannot fail");
        render_default_value(output, field);
        render_deprecation(output, field);
        writeln!(output).expect("writing to String cannot fail");
    }
    writeln!(output, "}}\n").expect("writing to String cannot fail");
    Ok(())
}

fn required_name<'a>(value: &'a Value, description: &str) -> Result<&'a str, String> {
    value
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("Storefront {description} has no name"))
}

fn type_ref_sdl(type_ref: &Value) -> Result<String, String> {
    match type_ref.get("kind").and_then(Value::as_str) {
        Some("NON_NULL") => Ok(format!(
            "{}!",
            type_ref_sdl(
                type_ref
                    .get("ofType")
                    .ok_or_else(|| "Storefront NON_NULL type has no ofType".to_string())?,
            )?
        )),
        Some("LIST") => Ok(format!(
            "[{}]",
            type_ref_sdl(
                type_ref
                    .get("ofType")
                    .ok_or_else(|| "Storefront LIST type has no ofType".to_string())?,
            )?
        )),
        Some("SCALAR" | "OBJECT" | "INTERFACE" | "UNION" | "ENUM" | "INPUT_OBJECT") => type_ref
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "Storefront named type has no name".to_string()),
        Some(kind) => Err(format!("unsupported Storefront type-reference kind {kind}")),
        None => Err("Storefront type reference has no kind".to_string()),
    }
}

fn render_default_value(output: &mut String, value: &Value) {
    if let Some(default_value) = value.get("defaultValue").and_then(Value::as_str) {
        write!(output, " = {default_value}").expect("writing to String cannot fail");
    }
}

fn render_deprecation(output: &mut String, value: &Value) {
    if value.get("isDeprecated").and_then(Value::as_bool) != Some(true) {
        return;
    }
    match value.get("deprecationReason").and_then(Value::as_str) {
        Some(reason) => write!(
            output,
            " @deprecated(reason: {})",
            serde_json::to_string(reason).expect("JSON string serialization cannot fail")
        )
        .expect("writing to String cannot fail"),
        None => write!(output, " @deprecated").expect("writing to String cannot fail"),
    }
}

fn named_type_from_display(field_type: &str) -> Option<String> {
    let named = field_type
        .trim_matches('!')
        .trim_matches('[')
        .trim_matches(']')
        .trim_matches('!');
    (!named.is_empty()).then(|| named.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::admin_graphql::{
        RootExecutionContext, RootFieldExecutor, RootFieldInvocation, RootFieldResult,
    };
    use std::sync::Arc;

    struct StaticExecutor;

    impl RootFieldExecutor for StaticExecutor {
        fn execute_root(&self, invocation: RootFieldInvocation) -> Result<RootFieldResult, String> {
            match invocation.root_name.as_str() {
                "shop" => Ok(RootFieldResult {
                    value: serde_json::json!({ "name": "Storefront schema shop" }),
                    errors: Vec::new(),
                    value_source: crate::admin_graphql::ResolverValueSource::Local,
                }),
                "search" => Ok(RootFieldResult {
                    value: serde_json::json!({
                        "nodes": [{
                            "__typename": "Product",
                            "id": "gid://shopify/Product/1",
                            "title": "Search result product"
                        }]
                    }),
                    errors: Vec::new(),
                    value_source: crate::admin_graphql::ResolverValueSource::Local,
                }),
                root => Err(format!(
                    "Storefront root `{root}` is not implemented locally"
                )),
            }
        }
    }

    #[test]
    fn every_captured_storefront_version_builds_and_introspects() {
        for version in StorefrontApiVersion::ALL {
            let schema = schema(version).unwrap_or_else(|error| {
                panic!("{version} should build as an executable Storefront schema: {error}")
            });
            let response = futures_executor::block_on(
                schema.execute("{ __schema { queryType { name } mutationType { name } } }"),
            );
            assert!(response.errors.is_empty(), "{:?}", response.errors);
            assert_eq!(
                response.data.into_json().unwrap(),
                serde_json::json!({
                    "__schema": {
                        "queryType": { "name": "QueryRoot" },
                        "mutationType": { "name": "Mutation" }
                    }
                })
            );
        }
    }

    #[test]
    fn storefront_schema_executes_storefront_types_independently_from_admin() {
        let schema = schema(StorefrontApiVersion::V2026_04).unwrap();
        let storefront = async_graphql::Request::new("{ shop { name } }")
            .data(RootExecutionContext::new(Arc::new(StaticExecutor)));
        let response = futures_executor::block_on(schema.execute(storefront));
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(
            response.data.into_json().unwrap(),
            serde_json::json!({ "shop": { "name": "Storefront schema shop" } })
        );

        let admin_only = futures_executor::block_on(schema.execute("{ productsCount { count } }"));
        assert!(admin_only
            .errors
            .iter()
            .any(|error| error.message.contains("Unknown field \"productsCount\"")));
    }

    #[test]
    fn storefront_schema_projects_concrete_fragments_from_search_result_union() {
        let schema = schema(StorefrontApiVersion::V2026_04).unwrap();
        let query = r#"
            {
              search(first: 1, query: "product") {
                nodes {
                  ...SearchResultFields
                }
              }
            }

            fragment SearchResultFields on SearchResultItem {
              __typename
              ... on Product { id title }
            }
            "#;
        let request =
            async_graphql::Request::new(expand_dynamic_union_fragment_spreads(schema, query))
                .data(RootExecutionContext::new(Arc::new(StaticExecutor)));
        let response = futures_executor::block_on(schema.execute(request));
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(
            response.data.into_json().unwrap(),
            serde_json::json!({
                "search": {
                    "nodes": [{
                        "__typename": "Product",
                        "id": "gid://shopify/Product/1",
                        "title": "Search result product"
                    }]
                }
            })
        );
    }
}
