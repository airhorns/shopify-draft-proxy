use std::collections::BTreeMap;

use graphql_parser::query::{
    parse_query, Definition, Document, Field, OperationDefinition, Selection, Type, TypeCondition,
    Value,
};
use graphql_parser::Pos;

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    List(Vec<ResolvedValue>),
    Object(BTreeMap<String, ResolvedValue>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RawArgumentValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    Enum(String),
    List(Vec<RawArgumentValue>),
    Object(BTreeMap<String, RawArgumentValue>),
    Variable {
        name: String,
        value: Option<ResolvedValue>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedDocument {
    pub operation_type: OperationType,
    pub operation_name: Option<String>,
    pub operation_path: String,
    pub location: SourceLocation,
    pub variable_definitions: BTreeMap<String, VariableDefinitionInfo>,
    pub root_fields: Vec<RootFieldSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedOperation {
    pub operation_type: OperationType,
    pub root_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectedField {
    pub name: String,
    pub response_key: String,
    pub location: SourceLocation,
    pub arguments: BTreeMap<String, ResolvedValue>,
    pub selection: Vec<SelectedField>,
    pub type_condition: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RootFieldSelection {
    pub name: String,
    pub response_key: String,
    pub location: SourceLocation,
    pub directives: Vec<String>,
    pub raw_arguments: BTreeMap<String, RawArgumentValue>,
    pub arguments: BTreeMap<String, ResolvedValue>,
    pub selection: Vec<SelectedField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VariableDefinitionInfo {
    pub name: String,
    pub type_name: String,
    pub type_display: String,
    pub default_value: Option<ResolvedValue>,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationSelectionError {
    Parse,
    MultipleOperationsRequireOperationName,
    UnknownOperationName(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationSelection {
    pub operation_count: usize,
    pub operation_name: Option<String>,
    pub requires_filtered_document: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Query,
    Mutation,
    Subscription,
}

impl OperationType {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Subscription => "subscription",
        }
    }
}

impl ParsedOperation {
    pub fn primary_root_field(&self) -> Option<&str> {
        self.root_fields.first().map(String::as_str)
    }
}

impl RawArgumentValue {
    pub fn resolved_value(&self) -> ResolvedValue {
        match self {
            Self::String(value) => ResolvedValue::String(value.clone()),
            Self::Int(value) => ResolvedValue::Int(*value),
            Self::Float(value) => ResolvedValue::Float(*value),
            Self::Bool(value) => ResolvedValue::Bool(*value),
            Self::Null => ResolvedValue::Null,
            Self::Enum(value) => ResolvedValue::String(value.clone()),
            Self::List(values) => {
                ResolvedValue::List(values.iter().map(Self::resolved_value).collect())
            }
            Self::Object(fields) => ResolvedValue::Object(
                fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.resolved_value()))
                    .collect(),
            ),
            Self::Variable { value, .. } => value.clone().unwrap_or(ResolvedValue::Null),
        }
    }

    pub fn is_literal_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    pub fn is_unbound_variable(&self) -> bool {
        matches!(self, Self::Variable { value: None, .. })
    }
}

pub fn parse_operation(query: &str) -> Option<ParsedOperation> {
    parse_operation_with_operation_name(query, None).ok()
}

pub fn parse_operation_with_operation_name(
    query: &str,
    operation_name: Option<&str>,
) -> Result<ParsedOperation, OperationSelectionError> {
    let document = parsed_document_for_operation(query, &BTreeMap::new(), operation_name)?;
    Ok(ParsedOperation {
        operation_type: document.operation_type,
        root_fields: document
            .root_fields
            .into_iter()
            .map(|field| field.name)
            .collect(),
    })
}

pub fn parsed_document(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<ParsedDocument> {
    parsed_document_with_operation_name(query, variables, None)
}

pub fn parsed_document_with_operation_name(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    operation_name: Option<&str>,
) -> Option<ParsedDocument> {
    parsed_document_for_operation(query, variables, operation_name).ok()
}

pub fn parsed_document_for_operation(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    operation_name: Option<&str>,
) -> Result<ParsedDocument, OperationSelectionError> {
    let document = parse_query::<&str>(query).map_err(|_| OperationSelectionError::Parse)?;
    let fragments = fragment_selections(&document.definitions);
    let operation = select_operation_definition(&document.definitions, operation_name)?;

    let (operation_type, name, location, variable_definitions, selections) =
        operation_parts(operation);
    Ok(ParsedDocument {
        operation_type,
        operation_name: name.map(str::to_string),
        operation_path: operation_path(operation_type, name),
        location: source_location(location),
        variable_definitions: variable_definition_infos(variable_definitions),
        root_fields: root_field_selections(selections, variables, &fragments),
    })
}

pub fn selected_operation(
    query: &str,
    operation_name: Option<&str>,
) -> Result<OperationSelection, OperationSelectionError> {
    let document = parse_query::<&str>(query).map_err(|_| OperationSelectionError::Parse)?;
    let operation_count = operation_definitions(&document.definitions).len();
    let operation = select_operation_definition(&document.definitions, operation_name)?;
    Ok(OperationSelection {
        operation_count,
        operation_name: operation_parts(operation).1.map(str::to_string),
        requires_filtered_document: operation_count > 1,
    })
}

pub fn selected_operation_query(
    query: &str,
    operation_name: Option<&str>,
) -> Result<String, OperationSelectionError> {
    let document = parse_query::<String>(query)
        .map_err(|_| OperationSelectionError::Parse)?
        .into_static();
    let (selected_index, operation_count) =
        selected_owned_operation_index(&document.definitions, operation_name)?;
    if operation_count <= 1 {
        return Ok(query.to_string());
    }

    let definitions = document.definitions;
    let mut selected_definitions = vec![definitions[selected_index].clone()];
    selected_definitions.extend(definitions.into_iter().enumerate().filter_map(
        |(index, definition)| {
            (index != selected_index && matches!(definition, Definition::Fragment(_)))
                .then_some(definition)
        },
    ));
    Ok(Document {
        definitions: selected_definitions,
    }
    .to_string())
}

pub fn variables_with_operation_defaults(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
    operation_name: Option<&str>,
) -> Result<BTreeMap<String, ResolvedValue>, OperationSelectionError> {
    let document = parsed_document_for_operation(query, variables, operation_name)?;
    let mut resolved = variables.clone();
    for definition in document.variable_definitions.values() {
        if !resolved.contains_key(&definition.name) {
            if let Some(default_value) = &definition.default_value {
                resolved.insert(definition.name.clone(), default_value.clone());
            }
        }
    }
    Ok(resolved)
}

pub fn root_field_arguments(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    let root_field = primary_root_field(query, variables)?;
    Some(root_field.arguments)
}

pub fn root_fields(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Vec<RootFieldSelection>> {
    Some(parsed_document(query, variables)?.root_fields)
}

pub fn primary_root_field(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<RootFieldSelection> {
    root_fields(query, variables)?.into_iter().next()
}

pub fn variable_definition_info(
    query: &str,
    variable_name: &str,
) -> Option<VariableDefinitionInfo> {
    parsed_document(query, &BTreeMap::new())?
        .variable_definitions
        .get(variable_name)
        .cloned()
}

fn selected_fields<'a>(
    selections: &'a [Selection<'a, &'a str>],
    variables: &BTreeMap<String, ResolvedValue>,
    fragments: &FragmentSelections<'a>,
) -> Vec<SelectedField> {
    selections
        .iter()
        .flat_map(|selection| match selection {
            Selection::Field(field) => vec![SelectedField {
                name: field.name.to_string(),
                response_key: field.alias.unwrap_or(field.name).to_string(),
                location: source_location(field.position),
                arguments: field_arguments(field, variables),
                selection: selected_fields(&field.selection_set.items, variables, fragments),
                type_condition: None,
            }],
            Selection::InlineFragment(fragment) => {
                let type_condition = fragment
                    .type_condition
                    .as_ref()
                    .map(type_condition_name)
                    .map(str::to_string);
                with_type_condition(
                    selected_fields(&fragment.selection_set.items, variables, fragments),
                    type_condition,
                )
            }
            Selection::FragmentSpread(fragment) => fragments
                .get(fragment.fragment_name)
                .map(|fragment_selection| {
                    with_type_condition(
                        selected_fields(fragment_selection.selection_set, variables, fragments),
                        fragment_selection.type_condition.map(str::to_string),
                    )
                })
                .unwrap_or_default(),
        })
        .collect()
}

fn with_type_condition(
    fields: Vec<SelectedField>,
    type_condition: Option<String>,
) -> Vec<SelectedField> {
    let Some(type_condition) = type_condition else {
        return fields;
    };
    fields
        .into_iter()
        .map(|mut field| {
            if field.type_condition.is_none() {
                field.type_condition = Some(type_condition.clone());
            }
            field
        })
        .collect()
}

fn type_condition_name<'a>(condition: &TypeCondition<'a, &'a str>) -> &'a str {
    match condition {
        TypeCondition::On(name) => name,
    }
}

fn field_arguments<'a>(
    field: &Field<'a, &'a str>,
    variables: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, ResolvedValue> {
    raw_field_arguments(field, variables)
        .into_iter()
        .map(|(name, value)| (name, value.resolved_value()))
        .collect()
}

fn raw_field_arguments<'a>(
    field: &Field<'a, &'a str>,
    variables: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, RawArgumentValue> {
    field
        .arguments
        .iter()
        .map(|(name, value)| (name.to_string(), raw_argument_value(value, variables)))
        .collect()
}

struct FragmentSelection<'a> {
    type_condition: Option<&'a str>,
    selection_set: &'a [Selection<'a, &'a str>],
}

type FragmentSelections<'a> = BTreeMap<&'a str, FragmentSelection<'a>>;
type VariableDefinitions<'a> = &'a [graphql_parser::query::VariableDefinition<'a, &'a str>];
type OperationParts<'a> = (
    OperationType,
    Option<&'a str>,
    Pos,
    VariableDefinitions<'a>,
    &'a [Selection<'a, &'a str>],
);

fn fragment_selections<'a>(definitions: &'a [Definition<'a, &'a str>]) -> FragmentSelections<'a> {
    definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Fragment(fragment) => Some((
                fragment.name,
                FragmentSelection {
                    type_condition: Some(type_condition_name(&fragment.type_condition)),
                    selection_set: fragment.selection_set.items.as_slice(),
                },
            )),
            Definition::Operation(_) => None,
        })
        .collect()
}

fn operation_definitions<'a>(
    definitions: &'a [Definition<'a, &'a str>],
) -> Vec<&'a OperationDefinition<'a, &'a str>> {
    definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Operation(operation) => Some(operation),
            Definition::Fragment(_) => None,
        })
        .collect()
}

fn select_operation_definition<'a>(
    definitions: &'a [Definition<'a, &'a str>],
    operation_name: Option<&str>,
) -> Result<&'a OperationDefinition<'a, &'a str>, OperationSelectionError> {
    let operations = operation_definitions(definitions);
    match operation_name {
        Some(expected_name) => operations
            .into_iter()
            .find(|operation| operation_parts(operation).1 == Some(expected_name))
            .ok_or_else(|| {
                OperationSelectionError::UnknownOperationName(expected_name.to_string())
            }),
        None if operations.len() > 1 => {
            Err(OperationSelectionError::MultipleOperationsRequireOperationName)
        }
        None => operations
            .into_iter()
            .next()
            .ok_or(OperationSelectionError::Parse),
    }
}

fn selected_owned_operation_index(
    definitions: &[Definition<'static, String>],
    operation_name: Option<&str>,
) -> Result<(usize, usize), OperationSelectionError> {
    let operation_count = definitions
        .iter()
        .filter(|definition| matches!(definition, Definition::Operation(_)))
        .count();
    match operation_name {
        Some(expected_name) => definitions
            .iter()
            .enumerate()
            .find_map(|(index, definition)| match definition {
                Definition::Operation(operation)
                    if owned_operation_name(operation) == Some(expected_name) =>
                {
                    Some((index, operation_count))
                }
                Definition::Operation(_) | Definition::Fragment(_) => None,
            })
            .ok_or_else(|| {
                OperationSelectionError::UnknownOperationName(expected_name.to_string())
            }),
        None if operation_count > 1 => {
            Err(OperationSelectionError::MultipleOperationsRequireOperationName)
        }
        None => definitions
            .iter()
            .enumerate()
            .find_map(|(index, definition)| {
                matches!(definition, Definition::Operation(_)).then_some((index, operation_count))
            })
            .ok_or(OperationSelectionError::Parse),
    }
}

fn owned_operation_name<'a>(
    operation: &'a OperationDefinition<'static, String>,
) -> Option<&'a str> {
    match operation {
        OperationDefinition::SelectionSet(_) => None,
        OperationDefinition::Query(query) => query.name.as_deref(),
        OperationDefinition::Mutation(mutation) => mutation.name.as_deref(),
        OperationDefinition::Subscription(subscription) => subscription.name.as_deref(),
    }
}

fn operation_parts<'a>(operation: &'a OperationDefinition<'a, &'a str>) -> OperationParts<'a> {
    match operation {
        OperationDefinition::SelectionSet(selection_set) => (
            OperationType::Query,
            None,
            selection_set.span.0,
            &[],
            selection_set.items.as_slice(),
        ),
        OperationDefinition::Query(query) => (
            OperationType::Query,
            query.name,
            query.position,
            query.variable_definitions.as_slice(),
            query.selection_set.items.as_slice(),
        ),
        OperationDefinition::Mutation(mutation) => (
            OperationType::Mutation,
            mutation.name,
            mutation.position,
            mutation.variable_definitions.as_slice(),
            mutation.selection_set.items.as_slice(),
        ),
        OperationDefinition::Subscription(subscription) => (
            OperationType::Subscription,
            subscription.name,
            subscription.position,
            subscription.variable_definitions.as_slice(),
            subscription.selection_set.items.as_slice(),
        ),
    }
}

fn variable_definition_infos<'a>(
    definitions: &'a [graphql_parser::query::VariableDefinition<'a, &'a str>],
) -> BTreeMap<String, VariableDefinitionInfo> {
    definitions
        .iter()
        .map(|definition| {
            let type_display = graphql_type_display(&definition.var_type);
            let type_name = graphql_named_type(&definition.var_type).unwrap_or(&type_display);
            let default_value = definition
                .default_value
                .as_ref()
                .map(|value| raw_argument_value(value, &BTreeMap::new()).resolved_value());
            (
                definition.name.to_string(),
                VariableDefinitionInfo {
                    name: definition.name.to_string(),
                    type_name: type_name.to_string(),
                    type_display,
                    default_value,
                    location: source_location(definition.position),
                },
            )
        })
        .collect()
}

fn graphql_type_display<'a>(type_: &Type<'a, &'a str>) -> String {
    match type_ {
        Type::NamedType(name) => (*name).to_string(),
        Type::ListType(inner) => format!("[{}]", graphql_type_display(inner)),
        Type::NonNullType(inner) => format!("{}!", graphql_type_display(inner)),
    }
}

fn graphql_named_type<'a>(type_: &'a Type<'a, &'a str>) -> Option<&'a str> {
    match type_ {
        Type::NamedType(name) => Some(*name),
        Type::ListType(inner) | Type::NonNullType(inner) => graphql_named_type(inner),
    }
}

fn operation_path(operation_type: OperationType, name: Option<&str>) -> String {
    let operation_type = operation_type.keyword();
    name.map_or_else(
        || operation_type.to_string(),
        |name| format!("{operation_type} {name}"),
    )
}

fn root_field_selections<'a>(
    selections: &'a [Selection<'a, &'a str>],
    variables: &BTreeMap<String, ResolvedValue>,
    fragments: &FragmentSelections<'a>,
) -> Vec<RootFieldSelection> {
    let mut fields = Vec::new();
    collect_root_field_selections(selections, variables, fragments, &mut fields);
    fields
}

fn collect_root_field_selections<'a>(
    selections: &'a [Selection<'a, &'a str>],
    variables: &BTreeMap<String, ResolvedValue>,
    fragments: &FragmentSelections<'a>,
    fields: &mut Vec<RootFieldSelection>,
) {
    for selection in selections {
        match selection {
            Selection::Field(field) => fields.push(RootFieldSelection {
                name: field.name.to_string(),
                response_key: field.alias.unwrap_or(field.name).to_string(),
                location: source_location(field.position),
                directives: field
                    .directives
                    .iter()
                    .map(|directive| directive.name.to_string())
                    .collect(),
                raw_arguments: raw_field_arguments(field, variables),
                arguments: field_arguments(field, variables),
                selection: selected_fields(&field.selection_set.items, variables, fragments),
            }),
            Selection::InlineFragment(fragment) => collect_root_field_selections(
                &fragment.selection_set.items,
                variables,
                fragments,
                fields,
            ),
            Selection::FragmentSpread(fragment) => {
                if let Some(fragment_selection) = fragments.get(fragment.fragment_name) {
                    collect_root_field_selections(
                        fragment_selection.selection_set,
                        variables,
                        fragments,
                        fields,
                    );
                }
            }
        }
    }
}

fn raw_argument_value<'a>(
    value: &Value<'a, &'a str>,
    variables: &BTreeMap<String, ResolvedValue>,
) -> RawArgumentValue {
    match value {
        Value::Variable(name) => RawArgumentValue::Variable {
            name: name.to_string(),
            value: variables.get(*name).cloned(),
        },
        Value::Int(number) => RawArgumentValue::Int(number.as_i64().unwrap_or_default()),
        Value::Float(value) => RawArgumentValue::Float(*value),
        Value::String(value) => RawArgumentValue::String(value.to_string()),
        Value::Boolean(value) => RawArgumentValue::Bool(*value),
        Value::Null => RawArgumentValue::Null,
        Value::Enum(value) => RawArgumentValue::Enum(value.to_string()),
        Value::List(values) => RawArgumentValue::List(
            values
                .iter()
                .map(|value| raw_argument_value(value, variables))
                .collect(),
        ),
        Value::Object(fields) => RawArgumentValue::Object(
            fields
                .iter()
                .map(|(name, value)| (name.to_string(), raw_argument_value(value, variables)))
                .collect(),
        ),
    }
}

fn source_location(position: Pos) -> SourceLocation {
    SourceLocation {
        line: position.line,
        column: position.column,
    }
}
