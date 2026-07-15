//! Executable Shopify Storefront GraphQL schemas.
//!
//! Storefront captures are kept independently from Admin captures because the
//! two APIs intentionally reuse root and object names with different types and
//! semantics. The Storefront capture is authenticated introspection JSON; this
//! module renders that immutable type graph to SDL once and passes it through
//! the same dynamic-schema builder used by Admin.

use std::{fmt::Write as _, sync::OnceLock};

use async_graphql::dynamic::Schema;
use serde_json::Value;

use crate::{
    admin_graphql::{build_schema_from_sdl, SchemaBuildError},
    graphql::OperationType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StorefrontApiVersion {
    V2026_04,
}

impl StorefrontApiVersion {
    pub const ALL: [Self; 1] = [Self::V2026_04];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::V2026_04 => "2026-04",
        }
    }

    pub fn from_route(path: &str) -> Option<Self> {
        let version = path.strip_prefix("/api/")?.strip_suffix("/graphql.json")?;
        Self::parse(version)
    }

    pub fn parse(version: &str) -> Option<Self> {
        match version {
            "2026-04" => Some(Self::V2026_04),
            _ => None,
        }
    }

    fn introspection_capture(self) -> &'static str {
        match self {
            Self::V2026_04 => {
                include_str!("../config/storefront-graphql/2026-04/schema.json")
            }
        }
    }
}

impl std::fmt::Display for StorefrontApiVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

static SCHEMA_2026_04: OnceLock<Schema> = OnceLock::new();
static SDL_2026_04: OnceLock<Result<String, String>> = OnceLock::new();

pub fn schema(version: StorefrontApiVersion) -> Result<&'static Schema, SchemaBuildError> {
    let slot = match version {
        StorefrontApiVersion::V2026_04 => &SCHEMA_2026_04,
    };
    if let Some(schema) = slot.get() {
        return Ok(schema);
    }
    let built = build_schema_from_sdl(schema_sdl(version)?, "Storefront")?;
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

fn schema_sdl(version: StorefrontApiVersion) -> Result<&'static str, SchemaBuildError> {
    let slot = match version {
        StorefrontApiVersion::V2026_04 => &SDL_2026_04,
    };
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
        let storefront =
            async_graphql::Request::new("{ shop { name } }").data(RootExecutionContext {
                executor: Arc::new(StaticExecutor),
            });
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
}
