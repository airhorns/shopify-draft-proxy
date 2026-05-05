//// Mirrors `src/graphql/parse-operation.ts`.
////
//// Lightweight façade over the parser that extracts just the bits the
//// proxy's dispatcher needs from an operation document: operation type
//// (query/mutation), optional operation name, and the names of the
//// root selection fields.
////
//// Subscriptions are explicitly rejected — the TS proxy does not handle
//// them and neither does the port.

import gleam/list
import gleam/option.{type Option, None, Some}
import shopify_draft_proxy/graphql/ast.{
  type Definition, type Document, type Selection, Field, FragmentDefinition,
  Mutation, OperationDefinition, Query, SelectionSet, Subscription,
}
import shopify_draft_proxy/graphql/parser
import shopify_draft_proxy/graphql/source

/// Operation kinds the proxy actually supports. Subscriptions are absent
/// by design — see `parse_operation` for the rejection.
pub type GraphQLOperationType {
  QueryOperation
  MutationOperation
}

/// Result of summarising an operation document.
pub type ParsedOperation {
  ParsedOperation(
    type_: GraphQLOperationType,
    name: Option(String),
    root_fields: List(String),
  )
}

/// Reasons `parse_operation` can fail. Distinct from `parser.ParseError`
/// so callers can disambiguate "couldn't parse the document" from
/// "parsed fine but the document isn't usable here".
pub type ParseOperationError {
  ParseFailed(parser.ParseError)
  NoOperationFound
  UnsupportedOperation(String)
}

/// Parse `document` and summarise the first operation definition.
///
/// Mirrors `parseOperation` in `parse-operation.ts`: finds the first
/// `OperationDefinition`, rejects anything other than query/mutation,
/// and projects the root selection set down to the names of its
/// `FieldNode` selections (fragment spreads and inline fragments are
/// dropped, matching the TS `.filter(kind === Kind.FIELD)`).
pub fn parse_operation(
  document: String,
) -> Result(ParsedOperation, ParseOperationError) {
  case parser.parse(source.new(document)) {
    Error(err) -> Error(ParseFailed(err))
    Ok(doc) -> summarise(doc)
  }
}

fn summarise(doc: Document) -> Result(ParsedOperation, ParseOperationError) {
  case find_operation(doc.definitions) {
    None -> Error(NoOperationFound)
    Some(OperationDefinition(operation: Subscription, ..)) ->
      Error(UnsupportedOperation("subscription"))
    Some(OperationDefinition(operation: op, name: name, selection_set: ss, ..)) -> {
      let SelectionSet(selections: selections, ..) = ss
      Ok(ParsedOperation(
        type_: operation_type(op),
        name: option.map(name, fn(n) { n.value }),
        root_fields: root_field_names(selections),
      ))
    }
    // find_operation only returns OperationDefinition values, so
    // FragmentDefinition is unreachable. Pattern listed for exhaustiveness.
    Some(FragmentDefinition(..)) -> Error(NoOperationFound)
  }
}

/// First `OperationDefinition` in a list of definitions, if any.
pub fn find_operation(definitions: List(Definition)) -> Option(Definition) {
  case definitions {
    [] -> None
    [d, ..rest] ->
      case d {
        OperationDefinition(..) -> Some(d)
        _ -> find_operation(rest)
      }
  }
}

fn operation_type(op: ast.OperationType) -> GraphQLOperationType {
  case op {
    Query -> QueryOperation
    Mutation -> MutationOperation
    // Subscription is filtered out one level up; this branch is
    // unreachable but required for exhaustiveness.
    Subscription -> QueryOperation
  }
}

fn root_field_names(selections: List(Selection)) -> List(String) {
  list.filter_map(selections, fn(selection) {
    case selection {
      Field(name: name, ..) -> Ok(name.value)
      _ -> Error(Nil)
    }
  })
}
