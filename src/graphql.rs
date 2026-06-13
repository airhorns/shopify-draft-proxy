use std::collections::BTreeMap;

use graphql_parser::query::{
    parse_query, Definition, Field, OperationDefinition, Selection, Type, Value,
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
    pub arguments: BTreeMap<String, ResolvedValue>,
    pub selection: Vec<SelectedField>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableDefinitionInfo {
    pub name: String,
    pub type_name: String,
    pub type_display: String,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationType {
    Query,
    Mutation,
    Subscription,
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
    let document = parsed_document(query, &BTreeMap::new())?;
    Some(ParsedOperation {
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
    let document = parse_query::<&str>(query).ok()?;
    let fragments = fragment_selections(&document.definitions);
    let operation = document
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Operation(operation) => Some(operation),
            Definition::Fragment(_) => None,
        })
        .find(|operation| operation_name_matches(operation, operation_name))?;

    let (operation_type, name, location, variable_definitions, selections) =
        operation_parts(operation);
    Some(ParsedDocument {
        operation_type,
        operation_name: name.map(str::to_string),
        operation_path: operation_path(operation_type, name),
        location: source_location(location),
        variable_definitions: variable_definition_infos(variable_definitions),
        root_fields: root_field_selections(selections, variables, &fragments),
    })
}

pub fn root_field_arguments(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    let root_field = root_fields(query, variables)?.into_iter().next()?;
    Some(root_field.arguments)
}

pub fn root_fields(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Vec<RootFieldSelection>> {
    Some(parsed_document(query, variables)?.root_fields)
}

pub fn root_field_selection(query: &str) -> Option<Vec<SelectedField>> {
    let root_field = first_root_field(query)?;
    Some(root_field.selection)
}

pub fn root_field_response_key(query: &str) -> Option<String> {
    let root_field = first_root_field(query)?;
    Some(root_field.response_key)
}

pub fn nested_root_field_selection(query: &str, child_name: &str) -> Option<Vec<SelectedField>> {
    nested_root_field_path_selection(query, &[child_name])
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

pub fn nested_root_field_path_selection(query: &str, path: &[&str]) -> Option<Vec<SelectedField>> {
    let root_field = first_root_field(query)?;
    nested_selected_field(&root_field.selection, path).map(|field| field.selection.clone())
}

fn first_root_field(query: &str) -> Option<RootFieldSelection> {
    root_fields(query, &BTreeMap::new())?.into_iter().next()
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
                arguments: field_arguments(field, variables),
                selection: selected_fields(&field.selection_set.items, variables, fragments),
            }],
            Selection::InlineFragment(fragment) => {
                selected_fields(&fragment.selection_set.items, variables, fragments)
            }
            Selection::FragmentSpread(fragment) => fragments
                .get(fragment.fragment_name)
                .map(|selection_set| selected_fields(selection_set, variables, fragments))
                .unwrap_or_default(),
        })
        .collect()
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

type FragmentSelections<'a> = BTreeMap<&'a str, &'a [Selection<'a, &'a str>]>;
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
            Definition::Fragment(fragment) => {
                Some((fragment.name, fragment.selection_set.items.as_slice()))
            }
            Definition::Operation(_) => None,
        })
        .collect()
}

fn operation_name_matches<'a>(
    operation: &'a OperationDefinition<'a, &'a str>,
    expected_name: Option<&str>,
) -> bool {
    expected_name.is_none_or(|expected_name| operation_parts(operation).1 == Some(expected_name))
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
            (
                definition.name.to_string(),
                VariableDefinitionInfo {
                    name: definition.name.to_string(),
                    type_name: type_name.to_string(),
                    type_display,
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
    let operation_type = match operation_type {
        OperationType::Query => "query",
        OperationType::Mutation => "mutation",
        OperationType::Subscription => "subscription",
    };
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
                if let Some(selection_set) = fragments.get(fragment.fragment_name) {
                    collect_root_field_selections(selection_set, variables, fragments, fields);
                }
            }
        }
    }
}

fn nested_selected_field<'a>(
    selections: &'a [SelectedField],
    path: &[&str],
) -> Option<&'a SelectedField> {
    let (next, remaining) = path.split_first()?;
    selections.iter().find_map(|selection| match selection {
        field if field.name == *next && remaining.is_empty() => Some(field),
        field if field.name == *next => nested_selected_field(&field.selection, remaining),
        _ => None,
    })
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
