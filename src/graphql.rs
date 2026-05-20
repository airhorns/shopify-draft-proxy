use std::collections::BTreeMap;

use graphql_parser::query::{
    parse_query, Definition, Field, OperationDefinition, Selection, Value,
};

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedOperation {
    pub operation_type: OperationType,
    pub root_fields: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedField {
    pub name: String,
    pub response_key: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RootFieldSelection {
    pub name: String,
    pub response_key: String,
    pub arguments: BTreeMap<String, ResolvedValue>,
    pub selection: Vec<SelectedField>,
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

pub fn parse_operation(query: &str) -> Option<ParsedOperation> {
    let document = parse_query::<&str>(query).ok()?;

    document
        .definitions
        .into_iter()
        .find_map(|definition| match definition {
            Definition::Operation(operation) => parsed_operation_from_definition(operation),
            Definition::Fragment(_) => None,
        })
}

pub fn root_field_arguments(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<BTreeMap<String, ResolvedValue>> {
    let document = parse_query::<&str>(query).ok()?;
    let operation = document
        .definitions
        .into_iter()
        .find_map(|definition| match definition {
            Definition::Operation(operation) => Some(operation),
            Definition::Fragment(_) => None,
        })?;
    let root_field = first_field_selection(operation)?;

    Some(field_arguments(root_field, variables))
}

pub fn root_fields(
    query: &str,
    variables: &BTreeMap<String, ResolvedValue>,
) -> Option<Vec<RootFieldSelection>> {
    let document = parse_query::<&str>(query).ok()?;
    let operation = document
        .definitions
        .into_iter()
        .find_map(|definition| match definition {
            Definition::Operation(operation) => Some(operation),
            Definition::Fragment(_) => None,
        })?;

    Some(
        operation_field_selections(operation)
            .into_iter()
            .map(|field| RootFieldSelection {
                name: field.name.to_string(),
                response_key: field.alias.unwrap_or(field.name).to_string(),
                arguments: field_arguments(field.clone(), variables),
                selection: selected_fields(field.selection_set.items),
            })
            .collect(),
    )
}

pub fn root_field_selection(query: &str) -> Option<Vec<SelectedField>> {
    let root_field = first_root_field(query)?;
    Some(selected_fields(root_field.selection_set.items))
}

pub fn root_field_response_key(query: &str) -> Option<String> {
    let root_field = first_root_field(query)?;
    Some(root_field.alias.unwrap_or(root_field.name).to_string())
}

pub fn nested_root_field_selection(query: &str, child_name: &str) -> Option<Vec<SelectedField>> {
    nested_root_field_path_selection(query, &[child_name])
}

pub fn nested_root_field_path_selection(query: &str, path: &[&str]) -> Option<Vec<SelectedField>> {
    let root_field = first_root_field(query)?;
    nested_selection(root_field.selection_set.items, path)
}

fn first_root_field<'a>(query: &'a str) -> Option<Field<'a, &'a str>> {
    let document = parse_query::<&str>(query).ok()?;
    let operation = document
        .definitions
        .into_iter()
        .find_map(|definition| match definition {
            Definition::Operation(operation) => Some(operation),
            Definition::Fragment(_) => None,
        })?;
    first_field_selection(operation)
}

fn parsed_operation_from_definition<'a>(
    operation: OperationDefinition<'a, &'a str>,
) -> Option<ParsedOperation> {
    match operation {
        OperationDefinition::SelectionSet(selection_set) => Some(ParsedOperation {
            operation_type: OperationType::Query,
            root_fields: field_names(selection_set.items),
        }),
        OperationDefinition::Query(query) => Some(ParsedOperation {
            operation_type: OperationType::Query,
            root_fields: field_names(query.selection_set.items),
        }),
        OperationDefinition::Mutation(mutation) => Some(ParsedOperation {
            operation_type: OperationType::Mutation,
            root_fields: field_names(mutation.selection_set.items),
        }),
        OperationDefinition::Subscription(subscription) => Some(ParsedOperation {
            operation_type: OperationType::Subscription,
            root_fields: field_names(subscription.selection_set.items),
        }),
    }
}

fn field_names<'a>(selections: Vec<Selection<'a, &'a str>>) -> Vec<String> {
    selections
        .into_iter()
        .filter_map(|selection| match selection {
            Selection::Field(field) => Some(field.name.to_string()),
            Selection::FragmentSpread(_) | Selection::InlineFragment(_) => None,
        })
        .collect()
}

fn selected_fields<'a>(selections: Vec<Selection<'a, &'a str>>) -> Vec<SelectedField> {
    selections
        .into_iter()
        .filter_map(|selection| match selection {
            Selection::Field(field) => Some(SelectedField {
                name: field.name.to_string(),
                response_key: field.alias.unwrap_or(field.name).to_string(),
            }),
            Selection::FragmentSpread(_) | Selection::InlineFragment(_) => None,
        })
        .collect()
}

fn field_arguments<'a>(
    field: Field<'a, &'a str>,
    variables: &BTreeMap<String, ResolvedValue>,
) -> BTreeMap<String, ResolvedValue> {
    field
        .arguments
        .into_iter()
        .map(|(name, value)| (name.to_string(), resolve_value(value, variables)))
        .collect()
}

fn operation_field_selections<'a>(
    operation: OperationDefinition<'a, &'a str>,
) -> Vec<Field<'a, &'a str>> {
    let selections = match operation {
        OperationDefinition::SelectionSet(selection_set) => selection_set.items,
        OperationDefinition::Query(query) => query.selection_set.items,
        OperationDefinition::Mutation(mutation) => mutation.selection_set.items,
        OperationDefinition::Subscription(subscription) => subscription.selection_set.items,
    };

    selections
        .into_iter()
        .filter_map(|selection| match selection {
            Selection::Field(field) => Some(field),
            Selection::FragmentSpread(_) | Selection::InlineFragment(_) => None,
        })
        .collect()
}

fn nested_selection<'a>(
    selections: Vec<Selection<'a, &'a str>>,
    path: &[&str],
) -> Option<Vec<SelectedField>> {
    let (next, remaining) = path.split_first()?;
    selections
        .into_iter()
        .find_map(|selection| match selection {
            Selection::Field(field) if field.name == *next && remaining.is_empty() => {
                Some(selected_fields(field.selection_set.items))
            }
            Selection::Field(field) if field.name == *next => {
                nested_selection(field.selection_set.items, remaining)
            }
            Selection::Field(_) | Selection::FragmentSpread(_) | Selection::InlineFragment(_) => {
                None
            }
        })
}

fn first_field_selection<'a>(
    operation: OperationDefinition<'a, &'a str>,
) -> Option<Field<'a, &'a str>> {
    let selections = match operation {
        OperationDefinition::SelectionSet(selection_set) => selection_set.items,
        OperationDefinition::Query(query) => query.selection_set.items,
        OperationDefinition::Mutation(mutation) => mutation.selection_set.items,
        OperationDefinition::Subscription(subscription) => subscription.selection_set.items,
    };

    selections
        .into_iter()
        .find_map(|selection| match selection {
            Selection::Field(field) => Some(field),
            Selection::FragmentSpread(_) | Selection::InlineFragment(_) => None,
        })
}

fn resolve_value<'a>(
    value: Value<'a, &'a str>,
    variables: &BTreeMap<String, ResolvedValue>,
) -> ResolvedValue {
    match value {
        Value::Variable(name) => variables.get(name).cloned().unwrap_or(ResolvedValue::Null),
        Value::Int(number) => ResolvedValue::Int(number.as_i64().unwrap_or_default()),
        Value::Float(value) => ResolvedValue::Float(value),
        Value::String(value) => ResolvedValue::String(value),
        Value::Boolean(value) => ResolvedValue::Bool(value),
        Value::Null => ResolvedValue::Null,
        Value::Enum(value) => ResolvedValue::String(value.to_string()),
        Value::List(values) => ResolvedValue::List(
            values
                .into_iter()
                .map(|value| resolve_value(value, variables))
                .collect(),
        ),
        Value::Object(fields) => ResolvedValue::Object(
            fields
                .into_iter()
                .map(|(name, value)| (name.to_string(), resolve_value(value, variables)))
                .collect(),
        ),
    }
}
