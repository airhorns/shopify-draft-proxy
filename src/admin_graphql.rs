//! Executable Shopify Admin GraphQL schemas.
//!
//! The schema files in `config/admin-graphql` are captured from Shopify's
//! standard introspection endpoint.  This module turns those captures into
//! `async-graphql` dynamic schemas: the GraphQL engine, rather than proxy code,
//! owns operation selection, variable coercion, validation, fragments,
//! directives, aliases, and null propagation.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::{Arc, OnceLock},
};

use async_graphql::{
    dynamic::{
        Enum, EnumItem, Field, FieldFuture, FieldValue, InputObject, InputValue, Interface,
        InterfaceField, Object, Scalar, Schema, SchemaError, TypeRef, Union,
    },
    Error, ErrorExtensionValues, Name, PathSegment, Pos, ServerError, Value as GraphqlValue,
};
use async_graphql_parser::{
    parse_schema,
    types::{
        BaseType, ConstDirective, FieldDefinition, InputValueDefinition, Type as AstType, TypeKind,
        TypeSystemDefinition,
    },
};
use serde_json::Value;

use crate::graphql::OperationType;

/// Admin API versions for which this repository carries a complete captured
/// schema.  Routes outside this set are rejected before execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AdminApiVersion {
    V2025_01,
    V2025_10,
    V2026_01,
    V2026_04,
    V2026_07,
}

impl AdminApiVersion {
    pub const ALL: [Self; 5] = [
        Self::V2025_01,
        Self::V2025_10,
        Self::V2026_01,
        Self::V2026_04,
        Self::V2026_07,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::V2025_01 => "2025-01",
            Self::V2025_10 => "2025-10",
            Self::V2026_01 => "2026-01",
            Self::V2026_04 => "2026-04",
            Self::V2026_07 => "2026-07",
        }
    }

    pub fn from_route(path: &str) -> Option<Self> {
        let version = path
            .strip_prefix("/admin/api/")?
            .strip_suffix("/graphql.json")?;
        Self::parse(version)
    }

    pub fn parse(version: &str) -> Option<Self> {
        match version {
            "2025-01" => Some(Self::V2025_01),
            "2025-10" => Some(Self::V2025_10),
            "2026-01" => Some(Self::V2026_01),
            "2026-04" => Some(Self::V2026_04),
            "2026-07" => Some(Self::V2026_07),
            _ => None,
        }
    }

    pub fn schema_sdl(self) -> &'static str {
        match self {
            Self::V2025_01 => {
                include_str!("../config/admin-graphql/2025-01/schema.graphql")
            }
            Self::V2025_10 => {
                include_str!("../config/admin-graphql/2025-10/schema.graphql")
            }
            Self::V2026_01 => {
                include_str!("../config/admin-graphql/2026-01/schema.graphql")
            }
            Self::V2026_04 => {
                include_str!("../config/admin-graphql/2026-04/schema.graphql")
            }
            Self::V2026_07 => {
                include_str!("../config/admin-graphql/2026-07/schema.graphql")
            }
        }
    }
}

impl fmt::Display for AdminApiVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// A root resolver result before `async-graphql` applies the caller's output
/// selection.  Domain resolvers may report GraphQL execution errors alongside
/// partial data.
#[derive(Debug, Clone, Default)]
pub struct RootFieldResult {
    pub value: Value,
    pub errors: Vec<RootFieldError>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RootFieldError {
    pub message: String,
    pub extensions: BTreeMap<String, Value>,
    pub path: Option<Vec<PathSegment>>,
    pub locations: Vec<Pos>,
}

/// The GraphQL engine's validated invocation of one root field. Arguments are
/// the values that async-graphql actually coerced, including schema defaults;
/// domain execution must not reinterpret the caller's raw variable JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct RootFieldInvocation {
    pub response_key: String,
    pub root_name: String,
    pub arguments: BTreeMap<String, Value>,
}

/// Request-scoped bridge between schema resolvers and the instance-owned
/// proxy/store. Implementations are expected to serialize mutation roots.
pub trait RootFieldExecutor: Send + Sync {
    fn execute_root(&self, invocation: RootFieldInvocation) -> Result<RootFieldResult, String>;
}

/// Put this value in `async_graphql::Request::data` when executing a proxy
/// request. Keeping the executor request-scoped prevents any global mutable
/// proxy state.
#[derive(Clone)]
pub struct RootExecutionContext {
    pub executor: Arc<dyn RootFieldExecutor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputKind {
    Scalar,
    Enum,
    Object,
    Interface,
    Union,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScalarCodec {
    ArbitraryJson,
    BigInteger,
    Decimal,
    Rfc3339DateTime,
    String,
    UnsignedInteger,
    Url,
}

impl ScalarCodec {
    fn accepts(self, value: &GraphqlValue) -> bool {
        match self {
            Self::ArbitraryJson => true,
            Self::BigInteger => match value {
                GraphqlValue::Number(number) => number.as_i64().is_some(),
                GraphqlValue::String(value) => value.parse::<i128>().is_ok(),
                _ => false,
            },
            Self::Decimal => valid_decimal_scalar(value),
            Self::Rfc3339DateTime => match value {
                GraphqlValue::String(value) => time::OffsetDateTime::parse(
                    value,
                    &time::format_description::well_known::Rfc3339,
                )
                .is_ok(),
                _ => false,
            },
            Self::String => matches!(value, GraphqlValue::String(_)),
            Self::UnsignedInteger => match value {
                GraphqlValue::Number(number) => number.as_u64().is_some(),
                GraphqlValue::String(value) => value.parse::<u64>().is_ok(),
                _ => false,
            },
            Self::Url => match value {
                GraphqlValue::String(value) => invalid_url_scalar_message(value).is_none(),
                _ => false,
            },
        }
    }
}

/// Return Shopify's URL scalar coercion message for values that cannot be
/// parsed. Domain validation (for example, HTTPS-only fields) happens after
/// this scalar boundary and therefore is intentionally not represented here.
pub(crate) fn invalid_url_scalar_message(value: &str) -> Option<String> {
    match url::Url::parse(value) {
        Ok(_) => None,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            Some(format!("Invalid url '{value}', missing scheme"))
        }
        Err(url::ParseError::EmptyHost) => Some(format!("Invalid url '{value}', missing host")),
        Err(_) if !value.contains(':') => Some(format!("Invalid url '{value}', missing scheme")),
        Err(_) => Some(format!("Invalid url '{value}'")),
    }
}

fn scalar_codec(name: &str) -> Option<ScalarCodec> {
    match name {
        "JSON" => Some(ScalarCodec::ArbitraryJson),
        "BigInt" => Some(ScalarCodec::BigInteger),
        "Decimal" | "Money" => Some(ScalarCodec::Decimal),
        "DateTime" | "ISO8601DateTime" => Some(ScalarCodec::Rfc3339DateTime),
        "UnsignedInt64" => Some(ScalarCodec::UnsignedInteger),
        "URL" => Some(ScalarCodec::Url),
        "ARN" | "Color" | "Date" | "FormattedString" | "HTML" | "StorefrontID" | "UtcOffset" => {
            Some(ScalarCodec::String)
        }
        _ => None,
    }
}

#[derive(Debug, Default)]
struct SchemaMetadata {
    output_kinds: BTreeMap<String, OutputKind>,
    possible_types: BTreeMap<String, BTreeSet<String>>,
    object_fields: BTreeMap<String, BTreeSet<String>>,
    input_fields: BTreeMap<String, BTreeMap<String, InputCoercionField>>,
}

#[derive(Debug, Clone)]
struct InputCoercionField {
    value_type: AstType,
}

#[derive(Debug)]
struct JsonObject(Value);

static SCHEMA_2025_01: OnceLock<Schema> = OnceLock::new();
static SCHEMA_2025_10: OnceLock<Schema> = OnceLock::new();
static SCHEMA_2026_01: OnceLock<Schema> = OnceLock::new();
static SCHEMA_2026_04: OnceLock<Schema> = OnceLock::new();
static SCHEMA_2026_07: OnceLock<Schema> = OnceLock::new();
static INPUT_OBJECT_FIELDS_2025_01: OnceLock<BTreeMap<String, Vec<InputFieldMetadata>>> =
    OnceLock::new();
static INPUT_OBJECT_FIELDS_2025_10: OnceLock<BTreeMap<String, Vec<InputFieldMetadata>>> =
    OnceLock::new();
static INPUT_OBJECT_FIELDS_2026_01: OnceLock<BTreeMap<String, Vec<InputFieldMetadata>>> =
    OnceLock::new();
static INPUT_OBJECT_FIELDS_2026_04: OnceLock<BTreeMap<String, Vec<InputFieldMetadata>>> =
    OnceLock::new();
static INPUT_OBJECT_FIELDS_2026_07: OnceLock<BTreeMap<String, Vec<InputFieldMetadata>>> =
    OnceLock::new();

/// Return the lazily-built executable schema for a Shopify API version.
pub fn schema(version: AdminApiVersion) -> Result<&'static Schema, SchemaBuildError> {
    let slot = match version {
        AdminApiVersion::V2025_01 => &SCHEMA_2025_01,
        AdminApiVersion::V2025_10 => &SCHEMA_2025_10,
        AdminApiVersion::V2026_01 => &SCHEMA_2026_01,
        AdminApiVersion::V2026_04 => &SCHEMA_2026_04,
        AdminApiVersion::V2026_07 => &SCHEMA_2026_07,
    };
    if let Some(schema) = slot.get() {
        return Ok(schema);
    }
    let built = build_schema(version)?;
    // Another request may have won the race. Either executable schema is built
    // from the same immutable capture, so retaining the winner is correct.
    let _ = slot.set(built);
    Ok(slot
        .get()
        .expect("versioned Admin GraphQL schema should be initialized"))
}

/// Test whether a concrete output type satisfies an interface or union type
/// condition in an executable Admin schema. Legacy JSON projection helpers use
/// this only while domain handlers are being migrated to return unprojected
/// resolver values; the GraphQL engine remains the final authority.
pub(crate) fn output_type_condition_applies(concrete_type: &str, type_condition: &str) -> bool {
    if concrete_type == type_condition {
        return true;
    }
    let initialized = [
        SCHEMA_2025_01.get(),
        SCHEMA_2025_10.get(),
        SCHEMA_2026_01.get(),
        SCHEMA_2026_04.get(),
        SCHEMA_2026_07.get(),
    ];
    let mut saw_initialized_schema = false;
    for schema in initialized.into_iter().flatten() {
        saw_initialized_schema = true;
        if schema_type_condition_applies(schema, concrete_type, type_condition) {
            return true;
        }
    }
    if saw_initialized_schema {
        return false;
    }
    schema(AdminApiVersion::V2026_07)
        .ok()
        .is_some_and(|schema| schema_type_condition_applies(schema, concrete_type, type_condition))
}

fn schema_type_condition_applies(
    schema: &Schema,
    concrete_type: &str,
    type_condition: &str,
) -> bool {
    if schema
        .registry()
        .implements
        .get(concrete_type)
        .is_some_and(|interfaces| interfaces.contains(type_condition))
    {
        return true;
    }
    matches!(
        schema.registry().types.get(type_condition),
        Some(
            async_graphql::registry::MetaType::Interface { possible_types, .. }
                | async_graphql::registry::MetaType::Union { possible_types, .. }
        ) if possible_types.contains(concrete_type)
    )
}

/// Look up an output field's named type from the same executable schema used
/// for requests. Domain helpers use this for nested bulk-query planning without
/// maintaining a second schema model.
pub fn output_field_named_type(
    version: &str,
    parent_type: &str,
    field_name: &str,
) -> Option<String> {
    let version = AdminApiVersion::parse(version)?;
    let field = schema(version)
        .ok()?
        .registry()
        .types
        .get(parent_type)?
        .field_by_name(field_name)?;
    let field_type = AstType::new(&field.ty)?;
    Some(named_type(&field_type).to_string())
}

/// Test an enum value against the exact captured versioned schema.
pub fn enum_value_allowed(version: &str, enum_name: &str, value: &str) -> bool {
    let Some(version) = AdminApiVersion::parse(version) else {
        return false;
    };
    matches!(
        schema(version)
            .ok()
            .and_then(|schema| schema.registry().types.get(enum_name)),
        Some(async_graphql::registry::MetaType::Enum { enum_values, .. })
            if enum_values.contains_key(value)
    )
}

/// Input metadata exposed from the executable registry so error-envelope
/// compatibility code does not need a second JSON schema model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFieldMetadata {
    pub name: String,
    pub type_display: String,
    pub named_type: String,
    pub required: bool,
    pub list: bool,
    pub list_item_required: bool,
}

fn input_field_metadata(
    input: &async_graphql::registry::MetaInputValue,
) -> Option<InputFieldMetadata> {
    let field_type = AstType::new(&input.ty)?;
    let (list, list_item_required) = match &field_type.base {
        BaseType::List(item) => (true, !item.nullable),
        BaseType::Named(_) => (false, false),
    };
    Some(InputFieldMetadata {
        name: input.name.clone(),
        type_display: input.ty.clone(),
        named_type: named_type(&field_type).to_string(),
        required: !field_type.nullable && input.default_value.is_none(),
        list,
        list_item_required,
    })
}

/// Return the captured fields of an input object in schema declaration order.
pub fn input_object_fields(
    version: AdminApiVersion,
    input_object_name: &str,
) -> Option<Vec<InputFieldMetadata>> {
    captured_input_object_fields(version)
        .get(input_object_name)
        .cloned()
}

fn captured_input_object_fields(
    version: AdminApiVersion,
) -> &'static BTreeMap<String, Vec<InputFieldMetadata>> {
    let slot = match version {
        AdminApiVersion::V2025_01 => &INPUT_OBJECT_FIELDS_2025_01,
        AdminApiVersion::V2025_10 => &INPUT_OBJECT_FIELDS_2025_10,
        AdminApiVersion::V2026_01 => &INPUT_OBJECT_FIELDS_2026_01,
        AdminApiVersion::V2026_04 => &INPUT_OBJECT_FIELDS_2026_04,
        AdminApiVersion::V2026_07 => &INPUT_OBJECT_FIELDS_2026_07,
    };
    slot.get_or_init(|| {
        let Ok(document) = parse_schema(version.schema_sdl()) else {
            return BTreeMap::new();
        };
        document
            .definitions
            .iter()
            .filter_map(|definition| {
                let TypeSystemDefinition::Type(definition) = definition else {
                    return None;
                };
                let TypeKind::InputObject(input_object) = &definition.node.kind else {
                    return None;
                };
                let fields = input_object
                    .fields
                    .iter()
                    .map(|field| {
                        let field_type = &field.node.ty.node;
                        let (list, list_item_required) = match &field_type.base {
                            BaseType::List(item) => (true, !item.nullable),
                            BaseType::Named(_) => (false, false),
                        };
                        InputFieldMetadata {
                            name: field.node.name.node.to_string(),
                            type_display: field_type.to_string(),
                            named_type: named_type(field_type).to_string(),
                            required: !field_type.nullable && field.node.default_value.is_none(),
                            list,
                            list_item_required,
                        }
                    })
                    .collect::<Vec<_>>();
                Some((definition.node.name.node.to_string(), fields))
            })
            .collect()
    })
}

/// Return captured enum values in schema declaration order.
pub fn enum_values(version: AdminApiVersion, enum_name: &str) -> Option<Vec<String>> {
    let async_graphql::registry::MetaType::Enum { enum_values, .. } =
        schema(version).ok()?.registry().types.get(enum_name)?
    else {
        return None;
    };
    Some(enum_values.keys().cloned().collect())
}

fn operation_root_name(schema: &Schema, operation_type: OperationType) -> Option<&str> {
    match operation_type {
        OperationType::Query => Some(schema.registry().query_type.as_str()),
        OperationType::Mutation => schema.registry().mutation_type.as_deref(),
        OperationType::Subscription => schema.registry().subscription_type.as_deref(),
    }
}

fn root_argument_metadata(
    version: AdminApiVersion,
    operation_type: OperationType,
    root_name: &str,
    argument_name: &str,
) -> Option<InputFieldMetadata> {
    let schema = schema(version).ok()?;
    let operation_root = operation_root_name(schema, operation_type)?;
    let field = schema
        .registry()
        .types
        .get(operation_root)?
        .field_by_name(root_name)?;
    input_field_metadata(field.args.get(argument_name)?)
}

/// Return a root field's argument definitions in captured schema order.
pub fn root_field_arguments(
    version: AdminApiVersion,
    operation_type: OperationType,
    root_name: &str,
) -> Option<Vec<InputFieldMetadata>> {
    let schema = schema(version).ok()?;
    let operation_root = operation_root_name(schema, operation_type)?;
    let field = schema
        .registry()
        .types
        .get(operation_root)?
        .field_by_name(root_name)?;
    field.args.values().map(input_field_metadata).collect()
}

/// Resolve a root argument or nested input field from a dotted argument path
/// such as `input.variants.0.price`. Numeric list indexes do not change the
/// named input type.
pub fn input_field_at_path(
    version: AdminApiVersion,
    operation_type: OperationType,
    root_name: &str,
    path: &[&str],
) -> Option<InputFieldMetadata> {
    let (argument_name, nested_path) = path.split_first()?;
    let mut field = root_argument_metadata(version, operation_type, root_name, argument_name)?;
    for segment in nested_path {
        if segment.parse::<usize>().is_ok() {
            continue;
        }
        field = input_object_fields(version, &field.named_type)?
            .into_iter()
            .find(|candidate| candidate.name == *segment)?;
    }
    Some(field)
}

/// Resolve the input-object type that owns the last segment in a dotted input
/// path. A one-segment path is a root field argument and therefore has no
/// InputObject owner.
pub fn input_owner_at_path(
    version: AdminApiVersion,
    operation_type: OperationType,
    root_name: &str,
    path: &[&str],
) -> Option<String> {
    if path.len() < 2 {
        return None;
    }
    let parent = input_field_at_path(version, operation_type, root_name, &path[..path.len() - 1])?;
    Some(parent.named_type)
}

fn named_type(field_type: &AstType) -> &str {
    match &field_type.base {
        BaseType::Named(name) => name.as_str(),
        BaseType::List(item) => named_type(item),
    }
}

#[derive(Debug)]
pub enum SchemaBuildError {
    Parse(String),
    MissingSchemaDefinition,
    MissingQueryRoot,
    UnsupportedScalar(String),
    Build(SchemaError),
}

impl fmt::Display for SchemaBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "could not parse captured schema: {error}"),
            Self::MissingSchemaDefinition => {
                formatter.write_str("captured schema has no schema definition")
            }
            Self::MissingQueryRoot => formatter.write_str("captured schema has no query root"),
            Self::UnsupportedScalar(name) => {
                write!(
                    formatter,
                    "captured schema uses unregistered scalar `{name}`"
                )
            }
            Self::Build(error) => write!(formatter, "could not build executable schema: {error}"),
        }
    }
}

impl std::error::Error for SchemaBuildError {}

impl From<SchemaError> for SchemaBuildError {
    fn from(error: SchemaError) -> Self {
        Self::Build(error)
    }
}

fn build_schema(version: AdminApiVersion) -> Result<Schema, SchemaBuildError> {
    build_schema_from_sdl(version.schema_sdl(), "Admin")
}

/// Build one executable Shopify GraphQL schema from captured SDL. Admin and
/// Storefront keep independent version inventories and caches, but share this
/// type/resolver construction so adding another API surface does not duplicate
/// the GraphQL machinery.
pub(crate) fn build_schema_from_sdl(
    schema_sdl: &str,
    api_surface: &'static str,
) -> Result<Schema, SchemaBuildError> {
    let document =
        parse_schema(schema_sdl).map_err(|error| SchemaBuildError::Parse(error.to_string()))?;
    let schema_definition = document
        .definitions
        .iter()
        .find_map(|definition| match definition {
            TypeSystemDefinition::Schema(definition) if !definition.node.extend => {
                Some(&definition.node)
            }
            _ => None,
        })
        .ok_or(SchemaBuildError::MissingSchemaDefinition)?;
    let query_root = schema_definition
        .query
        .as_ref()
        .map(|name| name.node.to_string())
        .ok_or(SchemaBuildError::MissingQueryRoot)?;
    let mutation_root = schema_definition
        .mutation
        .as_ref()
        .map(|name| name.node.to_string());
    let subscription_root = schema_definition
        .subscription
        .as_ref()
        .map(|name| name.node.to_string());

    let metadata = Arc::new(schema_metadata(&document.definitions));
    let mut builder = Schema::build(
        &query_root,
        mutation_root.as_deref(),
        subscription_root.as_deref(),
    );

    for definition in &document.definitions {
        let TypeSystemDefinition::Type(definition) = definition else {
            // async-graphql dynamic schemas do not currently expose a builder
            // for custom directive definitions. Schema-only
            // `@accessRestricted` is intentionally metadata-free here;
            // executable `@idempotent` is handled by the narrow request
            // preprocessor before engine validation.
            continue;
        };
        let name = definition.node.name.node.to_string();
        let description = definition
            .node
            .description
            .as_ref()
            .map(|description| description.node.clone());
        match &definition.node.kind {
            TypeKind::Scalar => {
                if matches!(name.as_str(), "Int" | "Float" | "String" | "Boolean" | "ID") {
                    continue;
                }
                let codec = scalar_codec(&name)
                    .ok_or_else(|| SchemaBuildError::UnsupportedScalar(name.clone()))?;
                let mut scalar = Scalar::new(name);
                scalar = scalar.validator(move |value| codec.accepts(value));
                if let Some(description) = description {
                    scalar = scalar.description(description);
                }
                if let Some(url) =
                    directive_string_argument(&definition.node.directives, "specifiedBy", "url")
                {
                    scalar = scalar.specified_by_url(url);
                }
                builder = builder.register(scalar);
            }
            TypeKind::Enum(enum_type) => {
                let mut graphql_enum = Enum::new(name);
                if let Some(description) = description {
                    graphql_enum = graphql_enum.description(description);
                }
                for value in &enum_type.values {
                    let mut item = EnumItem::new(value.node.value.node.to_string());
                    if let Some(description) = &value.node.description {
                        item = item.description(description.node.clone());
                    }
                    if let Some(reason) = deprecated_reason(&value.node.directives) {
                        item = item.deprecation(reason.as_deref());
                    }
                    graphql_enum = graphql_enum.item(item);
                }
                builder = builder.register(graphql_enum);
            }
            TypeKind::InputObject(input_object) => {
                let mut object = InputObject::new(name);
                if let Some(description) = description {
                    object = object.description(description);
                }
                for field in &input_object.fields {
                    object = object.field(dynamic_input_value(&field.node));
                }
                builder = builder.register(object);
            }
            TypeKind::Object(object_type) => {
                let is_root = name == query_root
                    || mutation_root.as_deref() == Some(name.as_str())
                    || subscription_root.as_deref() == Some(name.as_str());
                let mut object = Object::new(name.clone());
                if let Some(description) = description {
                    object = object.description(description);
                }
                for interface in &object_type.implements {
                    object = object.implement(interface.node.to_string());
                }
                for field in &object_type.fields {
                    object = object.field(if is_root {
                        dynamic_root_field(&field.node, Arc::clone(&metadata), api_surface)
                    } else {
                        dynamic_object_field(&name, &field.node, Arc::clone(&metadata))
                    });
                }
                builder = builder.register(object);
            }
            TypeKind::Interface(interface_type) => {
                let mut interface = Interface::new(name);
                if let Some(description) = description {
                    interface = interface.description(description);
                }
                for implemented in &interface_type.implements {
                    interface = interface.implement(implemented.node.to_string());
                }
                for field in &interface_type.fields {
                    let mut interface_field = InterfaceField::new(
                        field.node.name.node.to_string(),
                        dynamic_type_ref(&field.node.ty.node),
                    );
                    if let Some(description) = &field.node.description {
                        interface_field = interface_field.description(description.node.clone());
                    }
                    if let Some(reason) = deprecated_reason(&field.node.directives) {
                        interface_field = interface_field.deprecation(reason.as_deref());
                    }
                    for argument in &field.node.arguments {
                        interface_field =
                            interface_field.argument(dynamic_input_value(&argument.node));
                    }
                    interface = interface.field(interface_field);
                }
                builder = builder.register(interface);
            }
            TypeKind::Union(union_type) => {
                let mut union = Union::new(name);
                if let Some(description) = description {
                    union = union.description(description);
                }
                for member in &union_type.members {
                    union = union.possible_type(member.node.to_string());
                }
                builder = builder.register(union);
            }
        }
    }

    builder.finish().map_err(SchemaBuildError::Build)
}

fn valid_decimal_scalar(value: &GraphqlValue) -> bool {
    match value {
        GraphqlValue::Number(number) => number.as_f64().is_some_and(f64::is_finite),
        // Shopify accepts an empty Decimal string at the GraphQL scalar
        // boundary for inputs such as OrderCreateTaxLineInput.rate, then emits
        // the domain-level TAX_LINE_RATE_MISSING userError. Rejecting it here
        // changes both the error layer and the rest of a multi-root mutation.
        GraphqlValue::String(value) => {
            value.is_empty() || value.parse::<f64>().is_ok_and(f64::is_finite)
        }
        _ => false,
    }
}

fn schema_metadata(definitions: &[TypeSystemDefinition]) -> SchemaMetadata {
    let mut metadata = SchemaMetadata::default();
    let mut object_interfaces: Vec<(String, Vec<String>)> = Vec::new();
    for definition in definitions {
        let TypeSystemDefinition::Type(definition) = definition else {
            continue;
        };
        let name = definition.node.name.node.to_string();
        match &definition.node.kind {
            TypeKind::Scalar => {
                metadata.output_kinds.insert(name, OutputKind::Scalar);
            }
            TypeKind::Enum(_) => {
                metadata.output_kinds.insert(name, OutputKind::Enum);
            }
            TypeKind::Object(object) => {
                metadata
                    .output_kinds
                    .insert(name.clone(), OutputKind::Object);
                metadata.object_fields.insert(
                    name.clone(),
                    object
                        .fields
                        .iter()
                        .map(|field| field.node.name.node.to_string())
                        .collect(),
                );
                object_interfaces.push((
                    name,
                    object
                        .implements
                        .iter()
                        .map(|interface| interface.node.to_string())
                        .collect(),
                ));
            }
            TypeKind::Interface(_) => {
                metadata.output_kinds.insert(name, OutputKind::Interface);
            }
            TypeKind::Union(union) => {
                metadata
                    .output_kinds
                    .insert(name.clone(), OutputKind::Union);
                metadata.possible_types.insert(
                    name,
                    union
                        .members
                        .iter()
                        .map(|member| member.node.to_string())
                        .collect(),
                );
            }
            TypeKind::InputObject(input_object) => {
                metadata.input_fields.insert(
                    name,
                    input_object
                        .fields
                        .iter()
                        .map(|field| {
                            (
                                field.node.name.node.to_string(),
                                InputCoercionField {
                                    value_type: field.node.ty.node.clone(),
                                },
                            )
                        })
                        .collect(),
                );
            }
        }
    }
    for (object, interfaces) in object_interfaces {
        for interface in interfaces {
            metadata
                .possible_types
                .entry(interface)
                .or_default()
                .insert(object.clone());
        }
    }
    metadata
}

fn dynamic_root_field(
    field: &FieldDefinition,
    metadata: Arc<SchemaMetadata>,
    api_surface: &'static str,
) -> Field {
    let root_name = field.name.node.to_string();
    let output_type = dynamic_type_ref(&field.ty.node);
    let value_type = field.ty.node.clone();
    let argument_types = field
        .arguments
        .iter()
        .map(|argument| {
            (
                argument.node.name.node.to_string(),
                argument.node.ty.node.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let resolver_root_name = root_name.clone();
    let mut dynamic_field = Field::new(root_name, output_type, move |context| {
        let root_name = resolver_root_name.clone();
        let value_type = value_type.clone();
        let argument_types = argument_types.clone();
        let metadata = Arc::clone(&metadata);
        FieldFuture::new(async move {
            let execution = context.data::<RootExecutionContext>().map_err(|_| {
                Error::new(format!(
                    "{api_surface} GraphQL root `{root_name}` has no request-scoped resolver"
                ))
            })?;
            let response_key = context.field().alias().unwrap_or(&root_name).to_string();
            let arguments = context
                .args
                .iter()
                .map(|(name, value)| {
                    argument_types
                        .get(name.as_str())
                        .map_or_else(
                            || value.as_value().clone(),
                            |value_type| {
                                coerce_dynamic_input_value(
                                    value.as_value().clone(),
                                    value_type,
                                    &metadata,
                                )
                            },
                        )
                        .into_json()
                        .map(|value| (name.to_string(), value))
                        .map_err(|error| {
                            Error::new(format!(
                                "could not serialize coerced argument `{name}` for `{root_name}`: {error}"
                            ))
                        })
                })
                .collect::<Result<BTreeMap<_, _>, _>>()?;
            let result = match execution.executor.execute_root(RootFieldInvocation {
                response_key,
                root_name: root_name.clone(),
                arguments,
            }) {
                Ok(result) => result,
                Err(message) => {
                    context.add_error(context.set_error_path(ServerError::new(message, None)));
                    return Ok(FieldValue::NONE);
                }
            };
            for error in result.errors {
                let mut extensions = ErrorExtensionValues::default();
                for (name, value) in error.extensions {
                    if let Ok(value) = GraphqlValue::from_json(value) {
                        extensions.set(name, value);
                    }
                }
                let mut server_error = ServerError::new(error.message, None);
                server_error.extensions = Some(extensions);
                server_error.locations = error.locations;
                let server_error = if let Some(path) = error.path {
                    let mut server_error = context.set_error_path(server_error);
                    server_error.path.extend(path);
                    server_error
                } else {
                    server_error
                };
                context.add_error(server_error);
            }
            json_field_value(result.value, &value_type, &metadata)
        })
    });
    dynamic_field = decorate_field(dynamic_field, field);
    dynamic_field
}

fn coerce_dynamic_input_value(
    value: GraphqlValue,
    value_type: &AstType,
    metadata: &SchemaMetadata,
) -> GraphqlValue {
    if matches!(value, GraphqlValue::Null) {
        return value;
    }
    match &value_type.base {
        BaseType::List(item_type) => match value {
            GraphqlValue::List(values) => GraphqlValue::List(
                values
                    .into_iter()
                    .map(|value| coerce_dynamic_input_value(value, item_type, metadata))
                    .collect(),
            ),
            value => {
                GraphqlValue::List(vec![coerce_dynamic_input_value(value, item_type, metadata)])
            }
        },
        BaseType::Named(name) if name.as_str() == "ID" => match value {
            GraphqlValue::Number(value) => GraphqlValue::String(value.to_string()),
            value => value,
        },
        BaseType::Named(name) => {
            let Some(fields) = metadata.input_fields.get(name.as_str()) else {
                return value;
            };
            let GraphqlValue::Object(mut values) = value else {
                return value;
            };
            for (field_name, field) in fields {
                if let Some(value) = values.get_mut(field_name.as_str()) {
                    *value = coerce_dynamic_input_value(
                        std::mem::replace(value, GraphqlValue::Null),
                        &field.value_type,
                        metadata,
                    );
                }
            }
            GraphqlValue::Object(values)
        }
    }
}

fn dynamic_object_field(
    parent_type: &str,
    field: &FieldDefinition,
    metadata: Arc<SchemaMetadata>,
) -> Field {
    let parent_type = parent_type.to_string();
    let field_name = field.name.node.to_string();
    let resolver_field_name = field_name.clone();
    let value_type = field.ty.node.clone();
    let mut dynamic_field = Field::new(
        field_name,
        dynamic_type_ref(&field.ty.node),
        move |context| {
            let parent = match context.parent_value.try_downcast_ref::<JsonObject>() {
                Ok(parent) => parent,
                Err(error) => {
                    return FieldFuture::new(
                        async move { Err::<Option<FieldValue<'_>>, _>(error) },
                    );
                }
            };
            let response_key = context
                .field()
                .alias()
                .unwrap_or(&resolver_field_name)
                .to_string();
            let value = parent
                .0
                .as_object()
                .and_then(|object| {
                    object
                        .get(&response_key)
                        .or_else(|| object.get(&resolver_field_name))
                })
                .cloned();
            let Some(value) = value else {
                context.add_error(context.set_error_path(ServerError::new(
                    format!(
                        "Local resolver did not implement `{parent_type}.{resolver_field_name}`"
                    ),
                    None,
                )));
                return FieldFuture::Value(None);
            };
            let resolved = json_field_value(value, &value_type, &metadata);
            FieldFuture::new(async move { resolved })
        },
    );
    dynamic_field = decorate_field(dynamic_field, field);
    dynamic_field
}

fn decorate_field(mut dynamic_field: Field, field: &FieldDefinition) -> Field {
    if let Some(description) = &field.description {
        dynamic_field = dynamic_field.description(description.node.clone());
    }
    if let Some(reason) = deprecated_reason(&field.directives) {
        dynamic_field = dynamic_field.deprecation(reason.as_deref());
    }
    for argument in &field.arguments {
        dynamic_field = dynamic_field.argument(dynamic_input_value(&argument.node));
    }
    dynamic_field
}

fn dynamic_input_value(input: &InputValueDefinition) -> InputValue {
    let mut value = InputValue::new(
        input.name.node.to_string(),
        dynamic_type_ref(&input.ty.node),
    );
    if let Some(description) = &input.description {
        value = value.description(description.node.clone());
    }
    if let Some(default_value) = &input.default_value {
        value = value.default_value(default_value.node.clone());
    }
    if let Some(reason) = deprecated_reason(&input.directives) {
        value = value.deprecation(reason.as_deref());
    }
    value
}

fn dynamic_type_ref(ast: &AstType) -> TypeRef {
    let base = match &ast.base {
        BaseType::Named(name) => TypeRef::named(name.to_string()),
        BaseType::List(item) => TypeRef::List(Box::new(dynamic_type_ref(item))),
    };
    if ast.nullable {
        base
    } else {
        TypeRef::NonNull(Box::new(base))
    }
}

fn json_field_value<'a>(
    value: Value,
    value_type: &AstType,
    metadata: &SchemaMetadata,
) -> async_graphql::Result<Option<FieldValue<'a>>> {
    if value.is_null() {
        return Ok(FieldValue::NONE);
    }
    match &value_type.base {
        BaseType::List(item_type) => {
            let values = value.as_array().ok_or_else(|| {
                Error::new(format!(
                    "resolver returned a non-list value for `{value_type}`"
                ))
            })?;
            let mut items = Vec::with_capacity(values.len());
            for value in values {
                items.push(
                    json_field_value(value.clone(), item_type, metadata)?
                        .unwrap_or(FieldValue::NULL),
                );
            }
            Ok(Some(FieldValue::list(items)))
        }
        BaseType::Named(name) => match metadata.output_kinds.get(name.as_str()) {
            Some(OutputKind::Object) => Ok(Some(FieldValue::owned_any(JsonObject(value)))),
            Some(OutputKind::Interface | OutputKind::Union) => {
                let runtime_type =
                    infer_runtime_type(&value, name.as_str(), metadata).ok_or_else(|| {
                        Error::new(format!(
                            "resolver for abstract type `{name}` did not provide `__typename`"
                        ))
                    })?;
                Ok(Some(
                    FieldValue::owned_any(JsonObject(value)).with_type(runtime_type),
                ))
            }
            Some(OutputKind::Enum) => {
                let value = value.as_str().ok_or_else(|| {
                    Error::new(format!(
                        "resolver returned a non-string enum value for `{name}`"
                    ))
                })?;
                Ok(Some(FieldValue::value(GraphqlValue::Enum(Name::new(
                    value,
                )))))
            }
            Some(OutputKind::Scalar) | None => {
                let value = GraphqlValue::from_json(value).map_err(|error| {
                    Error::new(format!(
                        "could not convert resolver scalar `{name}`: {error}"
                    ))
                })?;
                Ok(Some(FieldValue::value(value)))
            }
        },
    }
}

fn infer_runtime_type(
    value: &Value,
    abstract_type: &str,
    metadata: &SchemaMetadata,
) -> Option<String> {
    let possible_types = metadata.possible_types.get(abstract_type)?;
    if let Some(type_name) = value.get("__typename").and_then(Value::as_str) {
        if possible_types.contains(type_name) {
            return Some(type_name.to_string());
        }
    }
    if let Some(type_name) = value
        .get("id")
        .and_then(Value::as_str)
        .and_then(shopify_gid_resource_type)
    {
        if possible_types.contains(type_name) {
            return Some(type_name.to_string());
        }
    }
    if possible_types.len() == 1 {
        return possible_types.iter().next().cloned();
    }

    // A number of Shopify value unions do not implement Node. Their concrete
    // object can still be identified unambiguously from the fields materialized
    // by the domain resolver.
    let selected_fields: BTreeSet<&str> = value
        .as_object()?
        .keys()
        .filter_map(|name| (!name.starts_with("__")).then_some(name.as_str()))
        .collect();
    let mut candidates = possible_types.iter().filter(|candidate| {
        metadata
            .object_fields
            .get(*candidate)
            .is_some_and(|fields| selected_fields.iter().all(|field| fields.contains(*field)))
    });
    let candidate = candidates.next()?.clone();
    candidates.next().is_none().then_some(candidate)
}

fn shopify_gid_resource_type(id: &str) -> Option<&str> {
    id.strip_prefix("gid://shopify/")?.split('/').next()
}

fn deprecated_reason(
    directives: &[async_graphql_parser::Positioned<ConstDirective>],
) -> Option<Option<String>> {
    directives.iter().find_map(|directive| {
        (directive.node.name.node.as_str() == "deprecated").then(|| {
            directive
                .node
                .arguments
                .iter()
                .find(|(name, _)| name.node.as_str() == "reason")
                .and_then(|(_, value)| match &value.node {
                    async_graphql::Value::String(reason) => Some(reason.clone()),
                    _ => None,
                })
        })
    })
}

fn directive_string_argument(
    directives: &[async_graphql_parser::Positioned<ConstDirective>],
    directive_name: &str,
    argument_name: &str,
) -> Option<String> {
    directives.iter().find_map(|directive| {
        (directive.node.name.node.as_str() == directive_name)
            .then(|| {
                directive
                    .node
                    .arguments
                    .iter()
                    .find(|(name, _)| name.node.as_str() == argument_name)
                    .and_then(|(_, value)| match &value.node {
                        async_graphql::Value::String(value) => Some(value.clone()),
                        _ => None,
                    })
            })
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_graphql::{Request, Variables};
    use futures_executor::block_on;

    struct StaticExecutor;

    #[derive(Clone)]
    struct RecordingExecutor(Arc<std::sync::Mutex<Option<RootFieldInvocation>>>);

    impl RootFieldExecutor for StaticExecutor {
        fn execute_root(&self, invocation: RootFieldInvocation) -> Result<RootFieldResult, String> {
            match invocation.root_name.as_str() {
                "shop" => Ok(RootFieldResult {
                    value: serde_json::json!({
                        "id": "gid://shopify/Shop/1",
                        "name": "Schema Shop"
                    }),
                    errors: Vec::new(),
                }),
                _ => Err(format!(
                    "root `{}` is not implemented locally",
                    invocation.root_name
                )),
            }
        }
    }

    impl RootFieldExecutor for RecordingExecutor {
        fn execute_root(&self, invocation: RootFieldInvocation) -> Result<RootFieldResult, String> {
            *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(invocation);
            Ok(RootFieldResult {
                value: serde_json::json!([]),
                errors: Vec::new(),
            })
        }
    }

    #[test]
    fn root_executor_receives_engine_coerced_arguments() {
        let recorded = Arc::new(std::sync::Mutex::new(None));
        let executor: Arc<dyn RootFieldExecutor> =
            Arc::new(RecordingExecutor(Arc::clone(&recorded)));
        let request = Request::new("query($ids: [ID!]!) { nodes(ids: $ids) { id } }")
            .variables(Variables::from_json(serde_json::json!({
                "ids": "gid://shopify/Product/1"
            })))
            .data(RootExecutionContext { executor });

        let response = block_on(schema(AdminApiVersion::V2026_04).unwrap().execute(request));
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        let invocation = recorded
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .expect("nodes resolver should run");
        assert_eq!(
            invocation.arguments.get("ids"),
            Some(&serde_json::json!(["gid://shopify/Product/1"]))
        );
    }

    #[test]
    fn every_captured_version_builds_and_introspects() {
        for version in AdminApiVersion::ALL {
            let schema = schema(version).unwrap_or_else(|error| {
                panic!("{version} should build as an executable schema: {error}")
            });
            let response = block_on(
                schema.execute("{ __schema { queryType { name } mutationType { name } } }"),
            );
            assert!(
                response.errors.is_empty(),
                "{version}: {:?}",
                response.errors
            );
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
    fn engine_rejects_fields_absent_from_a_version() {
        let operation = "{ __type(name: \"Mutation\") { fields { name } } }";
        let old = block_on(
            schema(AdminApiVersion::V2025_01)
                .unwrap()
                .execute(Request::new(operation)),
        );
        let current = block_on(
            schema(AdminApiVersion::V2026_04)
                .unwrap()
                .execute(Request::new(operation)),
        );
        let field_names = |response: async_graphql::Response| {
            response.data.into_json().unwrap()["__type"]["fields"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|field| field["name"].as_str())
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
        };
        assert!(!field_names(old).contains("channelCreate"));
        assert!(field_names(current).contains("channelCreate"));
    }

    #[test]
    fn route_parser_accepts_only_captured_versions() {
        assert_eq!(
            AdminApiVersion::from_route("/admin/api/2026-04/graphql.json"),
            Some(AdminApiVersion::V2026_04)
        );
        assert_eq!(
            AdminApiVersion::from_route("/admin/api/unstable/graphql.json"),
            None
        );
    }

    #[test]
    fn executable_versions_match_the_shared_manifest() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../config/admin-graphql/manifest.json"))
                .expect("Admin GraphQL version manifest should be valid JSON");
        let manifest_versions = manifest["executableVersions"]
            .as_array()
            .expect("manifest should list executableVersions")
            .iter()
            .map(|version| version.as_str().expect("version should be a string"))
            .collect::<Vec<_>>();
        assert_eq!(
            manifest_versions,
            AdminApiVersion::ALL
                .iter()
                .copied()
                .map(AdminApiVersion::as_str)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            manifest["defaultVersion"].as_str(),
            AdminApiVersion::ALL
                .last()
                .copied()
                .map(AdminApiVersion::as_str)
        );
    }

    #[test]
    fn engine_owns_fragments_aliases_variables_and_directives() {
        let request = Request::new(
            r#"
                query ShopThroughEngine($includeName: Boolean!) {
                  current: shop {
                    ...ShopIdentity
                    display: name @include(if: $includeName)
                  }
                }
                fragment ShopIdentity on Shop { id }
            "#,
        )
        .variables(Variables::from_json(
            serde_json::json!({ "includeName": true }),
        ))
        .data(RootExecutionContext {
            executor: Arc::new(StaticExecutor),
        });
        let response = block_on(schema(AdminApiVersion::V2026_04).unwrap().execute(request));
        assert!(response.errors.is_empty(), "{:?}", response.errors);
        assert_eq!(
            response.data.into_json().unwrap(),
            serde_json::json!({
                "current": {
                    "id": "gid://shopify/Shop/1",
                    "display": "Schema Shop"
                }
            })
        );
    }

    #[test]
    fn missing_root_resolvers_are_explicit_execution_errors() {
        let request = Request::new("{ product(id: \"gid://shopify/Product/1\") { id } }").data(
            RootExecutionContext {
                executor: Arc::new(StaticExecutor),
            },
        );
        let response = block_on(schema(AdminApiVersion::V2026_04).unwrap().execute(request));
        assert_eq!(
            response.errors.len(),
            1,
            "data={:?}, errors={:?}",
            response.data,
            response.errors
        );
        assert!(response.errors[0]
            .message
            .contains("root `product` is not implemented locally"));
        assert_eq!(response.errors[0].path.len(), 1);
    }

    #[test]
    fn missing_object_fields_are_explicit_execution_errors() {
        let request = Request::new("{ shop { myshopifyDomain } }").data(RootExecutionContext {
            executor: Arc::new(StaticExecutor),
        });
        let response = block_on(schema(AdminApiVersion::V2026_04).unwrap().execute(request));
        assert!(response.errors.iter().any(|error| error
            .message
            .contains("Local resolver did not implement `Shop.myshopifyDomain`")));
    }
}
