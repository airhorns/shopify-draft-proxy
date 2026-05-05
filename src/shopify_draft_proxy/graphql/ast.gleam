//// Mirrors the operation-document subset of `graphql-js` `language/ast.ts`.
////
//// The proxy only parses GraphQL *executable* documents (queries,
//// mutations, subscriptions, fragments) sent by clients. Schema-definition
//// nodes (`ScalarTypeDefinition`, `ObjectTypeDefinition`, type extensions,
//// directive definitions, etc.) are intentionally not represented here —
//// the Gleam port does not parse schema text.
////
//// Field naming differs from graphql-js where JS keywords or stylistic
//// preferences require it: `type` becomes `type_ref` / `inner` /
//// `type_condition`, camelCase becomes snake_case throughout.
//// `Location` only carries the start/end character offsets used by error
//// reporting; graphql-js additionally stores start/end token references,
//// which the parser does not need.

import gleam/option.{type Option}

/// Source range a node was parsed from. 0-indexed code-point offsets into
/// the originating `Source.body`. Mirrors graphql-js's `Location.start`
/// and `.end`; the `startToken`/`endToken` and `source` fields are omitted
/// because the proxy does not consume them.
pub type Location {
  Location(start: Int, end: Int)
}

/// `query`, `mutation`, or `subscription`.
pub type OperationType {
  Query
  Mutation
  Subscription
}

/// A `Name` — an identifier in the source. Mirrors `NameNode`.
pub type Name {
  Name(value: String, loc: Option(Location))
}

/// `$variable`. Used both standalone (in `VariableDefinition.variable`) and
/// nested inside a `Value` via `VariableValue`. graphql-js represents this
/// as a single `VariableNode`; Gleam needs a wrapper variant on `Value` to
/// fit the `VariableNode | …` union into a sum type.
pub type Variable {
  Variable(name: Name, loc: Option(Location))
}

/// Type references in variable definitions. Mirrors `TypeNode`. The name
/// `TypeRef` is used (not `Type`) to avoid colliding with the built-in
/// Gleam concept of "type".
pub type TypeRef {
  NamedType(name: Name, loc: Option(Location))
  ListType(inner: TypeRef, loc: Option(Location))
  NonNullType(inner: TypeRef, loc: Option(Location))
}

/// GraphQL value literal. Mirrors `ValueNode`. The `block` flag on
/// `StringValue` distinguishes single-quoted from triple-quoted strings,
/// matching graphql-js.
pub type Value {
  IntValue(value: String, loc: Option(Location))
  FloatValue(value: String, loc: Option(Location))
  StringValue(value: String, block: Bool, loc: Option(Location))
  BooleanValue(value: Bool, loc: Option(Location))
  NullValue(loc: Option(Location))
  EnumValue(value: String, loc: Option(Location))
  ListValue(values: List(Value), loc: Option(Location))
  ObjectValue(fields: List(ObjectField), loc: Option(Location))
  VariableValue(variable: Variable)
}

/// A single key/value pair inside an `ObjectValue`.
pub type ObjectField {
  ObjectField(name: Name, value: Value, loc: Option(Location))
}

/// `name: value`. Used inside arguments lists on fields and directives.
pub type Argument {
  Argument(name: Name, value: Value, loc: Option(Location))
}

/// `@name(arg: …)` decorating a field, fragment, definition, etc.
pub type Directive {
  Directive(name: Name, arguments: List(Argument), loc: Option(Location))
}

/// `$var: Type = default @directive`. Used inside operation definitions.
pub type VariableDefinition {
  VariableDefinition(
    variable: Variable,
    type_ref: TypeRef,
    default_value: Option(Value),
    directives: List(Directive),
    loc: Option(Location),
  )
}

/// One element of a selection set. Mirrors `SelectionNode`.
pub type Selection {
  Field(
    alias: Option(Name),
    name: Name,
    arguments: List(Argument),
    directives: List(Directive),
    selection_set: Option(SelectionSet),
    loc: Option(Location),
  )
  FragmentSpread(name: Name, directives: List(Directive), loc: Option(Location))
  InlineFragment(
    type_condition: Option(TypeRef),
    directives: List(Directive),
    selection_set: SelectionSet,
    loc: Option(Location),
  )
}

/// `{ … }`. Wraps the list of selections so it can carry its own location.
pub type SelectionSet {
  SelectionSet(selections: List(Selection), loc: Option(Location))
}

/// Top-level executable definitions. Mirrors `ExecutableDefinitionNode`.
pub type Definition {
  OperationDefinition(
    operation: OperationType,
    name: Option(Name),
    variable_definitions: List(VariableDefinition),
    directives: List(Directive),
    selection_set: SelectionSet,
    loc: Option(Location),
  )
  FragmentDefinition(
    name: Name,
    type_condition: TypeRef,
    directives: List(Directive),
    selection_set: SelectionSet,
    loc: Option(Location),
  )
}

/// A complete GraphQL document. Mirrors `DocumentNode`.
pub type Document {
  Document(definitions: List(Definition), loc: Option(Location))
}
