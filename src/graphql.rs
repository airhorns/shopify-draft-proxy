use graphql_parser::query::{parse_query, Definition, OperationDefinition, Selection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedOperation {
    pub operation_type: OperationType,
    pub root_fields: Vec<String>,
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
