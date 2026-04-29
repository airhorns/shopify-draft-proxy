//// Mirrors `graphql-js` `language/parser.ts`, scoped to executable
//// definitions (queries, mutations, subscriptions, fragments).
////
//// Schema-definition productions (`SchemaDefinition`, `*TypeDefinition`,
//// `*TypeExtension`, `DirectiveDefinition`, `SchemaCoordinate`) and the
//// legacy fragment-variables option are intentionally omitted — the proxy
//// does not parse schema text or rely on those legacy modes.
////
//// graphql-js mutates `Parser` and `Lexer` instances. This port threads
//// an immutable `Parser` value through every parsing function and uses
//// `Result` rather than thrown errors. Each `parse_*` function takes the
//// current parser, consumes some tokens, and returns the produced AST
//// node plus the advanced parser.

import gleam/list
import gleam/option.{None, Some}
import gleam/result
import shopify_draft_proxy/graphql/ast.{
  type Argument, type Definition, type Directive, type Document, type Name,
  type ObjectField, type Selection, type SelectionSet, type TypeRef, type Value,
  type Variable, type VariableDefinition, Argument, BooleanValue, Directive,
  Document, EnumValue, Field, FloatValue, FragmentDefinition, FragmentSpread,
  InlineFragment, IntValue, ListType, ListValue, Location, Mutation, Name,
  NamedType, NonNullType, NullValue, ObjectField, ObjectValue,
  OperationDefinition, Query, SelectionSet, StringValue, Subscription, Variable,
  VariableDefinition, VariableValue,
}
import shopify_draft_proxy/graphql/lexer
import shopify_draft_proxy/graphql/source.{type Source}
import shopify_draft_proxy/graphql/token.{type Token}
import shopify_draft_proxy/graphql/token_kind as tk

/// A parse-time error. Mirrors `graphql-js` `GraphQLError` for the parser.
pub type ParseError {
  ParseError(message: String, position: Int, line: Int, column: Int)
}

/// Immutable parser state. `token` is the *current* (lookahead) token;
/// `last_token` is the most recently consumed token, and is used to compute
/// the end of an AST node's location range.
pub type Parser {
  Parser(lexer: lexer.Lexer, token: Token, last_token: Token)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a `Source` containing an executable GraphQL document.
pub fn parse(source: Source) -> Result(Document, ParseError) {
  case new_parser(source) {
    Error(e) -> Error(e)
    Ok(parser) -> {
      let start = parser.token
      use #(definitions, parser) <- result.try(parse_definitions(parser, []))
      let loc = Some(Location(start: start.start, end: parser.last_token.end))
      Ok(Document(definitions: definitions, loc: loc))
    }
  }
}

fn new_parser(source: Source) -> Result(Parser, ParseError) {
  let lex_state = lexer.new(source)
  let sof = token.punctuator(tk.Sof, 0, 0, 1, 1)
  case lexer.next_token(lex_state) {
    Error(e) -> Error(lex_to_parse(e))
    Ok(#(t, after)) -> Ok(Parser(lexer: after, token: t, last_token: sof))
  }
}

// ---------------------------------------------------------------------------
// Document / Definition
// ---------------------------------------------------------------------------

fn parse_definitions(
  parser: Parser,
  acc: List(Definition),
) -> Result(#(List(Definition), Parser), ParseError) {
  case peek(parser, tk.Eof) {
    True -> Ok(#(list.reverse(acc), parser))
    False -> {
      use #(def, parser) <- result.try(parse_definition(parser))
      parse_definitions(parser, [def, ..acc])
    }
  }
}

fn parse_definition(
  parser: Parser,
) -> Result(#(Definition, Parser), ParseError) {
  case peek(parser, tk.BraceL) {
    True -> parse_operation_definition(parser)
    False ->
      case parser.token.kind == tk.Name {
        True ->
          case parser.token.value {
            Some("query") -> parse_operation_definition(parser)
            Some("mutation") -> parse_operation_definition(parser)
            Some("subscription") -> parse_operation_definition(parser)
            Some("fragment") -> parse_fragment_definition(parser)
            _ -> Error(unexpected(parser))
          }
        False -> Error(unexpected(parser))
      }
  }
}

fn parse_operation_definition(
  parser: Parser,
) -> Result(#(Definition, Parser), ParseError) {
  let start = parser.token
  case peek(parser, tk.BraceL) {
    True -> {
      use #(selection_set, parser) <- result.try(parse_selection_set(parser))
      let loc = Some(Location(start: start.start, end: parser.last_token.end))
      Ok(#(
        OperationDefinition(
          operation: Query,
          name: None,
          variable_definitions: [],
          directives: [],
          selection_set: selection_set,
          loc: loc,
        ),
        parser,
      ))
    }
    False -> {
      use #(operation, parser) <- result.try(parse_operation_type(parser))
      use #(name, parser) <- result.try(case peek(parser, tk.Name) {
        True -> {
          use #(n, parser) <- result.try(parse_name(parser))
          Ok(#(Some(n), parser))
        }
        False -> Ok(#(None, parser))
      })
      use #(var_defs, parser) <- result.try(parse_variable_definitions(parser))
      use #(directives, parser) <- result.try(parse_directives(parser, False))
      use #(selection_set, parser) <- result.try(parse_selection_set(parser))
      let loc = Some(Location(start: start.start, end: parser.last_token.end))
      Ok(#(
        OperationDefinition(
          operation: operation,
          name: name,
          variable_definitions: var_defs,
          directives: directives,
          selection_set: selection_set,
          loc: loc,
        ),
        parser,
      ))
    }
  }
}

fn parse_operation_type(
  parser: Parser,
) -> Result(#(ast.OperationType, Parser), ParseError) {
  let saved = parser.token
  use #(t, parser) <- result.try(expect_token(parser, tk.Name))
  case t.value {
    Some("query") -> Ok(#(Query, parser))
    Some("mutation") -> Ok(#(Mutation, parser))
    Some("subscription") -> Ok(#(Subscription, parser))
    _ -> Error(unexpected_token(saved))
  }
}

fn parse_fragment_definition(
  parser: Parser,
) -> Result(#(Definition, Parser), ParseError) {
  let start = parser.token
  use parser <- result.try(expect_keyword(parser, "fragment"))
  use #(name, parser) <- result.try(parse_fragment_name(parser))
  use parser <- result.try(expect_keyword(parser, "on"))
  use #(type_cond, parser) <- result.try(parse_named_type(parser))
  use #(directives, parser) <- result.try(parse_directives(parser, False))
  use #(ss, parser) <- result.try(parse_selection_set(parser))
  let loc = Some(Location(start: start.start, end: parser.last_token.end))
  Ok(#(
    FragmentDefinition(
      name: name,
      type_condition: type_cond,
      directives: directives,
      selection_set: ss,
      loc: loc,
    ),
    parser,
  ))
}

// ---------------------------------------------------------------------------
// Variable definitions
// ---------------------------------------------------------------------------

fn parse_variable_definitions(
  parser: Parser,
) -> Result(#(List(VariableDefinition), Parser), ParseError) {
  optional_many(parser, tk.ParenL, parse_variable_definition, tk.ParenR)
}

fn parse_variable_definition(
  parser: Parser,
) -> Result(#(VariableDefinition, Parser), ParseError) {
  let start = parser.token
  use #(variable, parser) <- result.try(parse_variable(parser))
  use #(_, parser) <- result.try(expect_token(parser, tk.Colon))
  use #(type_ref, parser) <- result.try(parse_type_reference(parser))
  use #(default, parser) <- result.try(case peek(parser, tk.Equals) {
    True -> {
      use #(_, parser) <- result.try(expect_token(parser, tk.Equals))
      use #(value, parser) <- result.try(parse_value_literal(parser, True))
      Ok(#(Some(value), parser))
    }
    False -> Ok(#(None, parser))
  })
  use #(directives, parser) <- result.try(parse_directives(parser, True))
  let loc = Some(Location(start: start.start, end: parser.last_token.end))
  Ok(#(
    VariableDefinition(
      variable: variable,
      type_ref: type_ref,
      default_value: default,
      directives: directives,
      loc: loc,
    ),
    parser,
  ))
}

fn parse_variable(parser: Parser) -> Result(#(Variable, Parser), ParseError) {
  let start = parser.token
  use #(_, parser) <- result.try(expect_token(parser, tk.Dollar))
  use #(name, parser) <- result.try(parse_name(parser))
  let loc = Some(Location(start: start.start, end: parser.last_token.end))
  Ok(#(Variable(name: name, loc: loc), parser))
}

// ---------------------------------------------------------------------------
// Selections / Fields / Fragments
// ---------------------------------------------------------------------------

fn parse_selection_set(
  parser: Parser,
) -> Result(#(SelectionSet, Parser), ParseError) {
  let start = parser.token
  use #(selections, parser) <- result.try(many(
    parser,
    tk.BraceL,
    parse_selection,
    tk.BraceR,
  ))
  let loc = Some(Location(start: start.start, end: parser.last_token.end))
  Ok(#(SelectionSet(selections: selections, loc: loc), parser))
}

fn parse_selection(parser: Parser) -> Result(#(Selection, Parser), ParseError) {
  case peek(parser, tk.Spread) {
    True -> parse_fragment(parser)
    False -> parse_field(parser)
  }
}

fn parse_field(parser: Parser) -> Result(#(Selection, Parser), ParseError) {
  let start = parser.token
  use #(name_or_alias, parser) <- result.try(parse_name(parser))
  use #(was_colon, parser) <- result.try(expect_optional_token(parser, tk.Colon))
  let alias_name = case was_colon {
    True -> {
      use #(real_name, parser) <- result.try(parse_name(parser))
      Ok(#(Some(name_or_alias), real_name, parser))
    }
    False -> Ok(#(None, name_or_alias, parser))
  }
  use #(alias, name, parser) <- result.try(alias_name)
  use #(args, parser) <- result.try(parse_arguments(parser, False))
  use #(directives, parser) <- result.try(parse_directives(parser, False))
  use #(selection_set, parser) <- result.try(case peek(parser, tk.BraceL) {
    True -> {
      use #(ss, parser) <- result.try(parse_selection_set(parser))
      Ok(#(Some(ss), parser))
    }
    False -> Ok(#(None, parser))
  })
  let loc = Some(Location(start: start.start, end: parser.last_token.end))
  Ok(#(
    Field(
      alias: alias,
      name: name,
      arguments: args,
      directives: directives,
      selection_set: selection_set,
      loc: loc,
    ),
    parser,
  ))
}

fn parse_fragment(parser: Parser) -> Result(#(Selection, Parser), ParseError) {
  let start = parser.token
  use #(_, parser) <- result.try(expect_token(parser, tk.Spread))
  use #(has_type_cond, parser) <- result.try(expect_optional_keyword(
    parser,
    "on",
  ))
  case !has_type_cond && peek(parser, tk.Name) {
    True -> {
      use #(name, parser) <- result.try(parse_fragment_name(parser))
      use #(directives, parser) <- result.try(parse_directives(parser, False))
      let loc = Some(Location(start: start.start, end: parser.last_token.end))
      Ok(#(FragmentSpread(name: name, directives: directives, loc: loc), parser))
    }
    False -> {
      use #(type_cond, parser) <- result.try(case has_type_cond {
        True -> {
          use #(t, parser) <- result.try(parse_named_type(parser))
          Ok(#(Some(t), parser))
        }
        False -> Ok(#(None, parser))
      })
      use #(directives, parser) <- result.try(parse_directives(parser, False))
      use #(ss, parser) <- result.try(parse_selection_set(parser))
      let loc = Some(Location(start: start.start, end: parser.last_token.end))
      Ok(#(
        InlineFragment(
          type_condition: type_cond,
          directives: directives,
          selection_set: ss,
          loc: loc,
        ),
        parser,
      ))
    }
  }
}

fn parse_fragment_name(parser: Parser) -> Result(#(Name, Parser), ParseError) {
  case parser.token.value == Some("on") {
    True -> Error(unexpected(parser))
    False -> parse_name(parser)
  }
}

// ---------------------------------------------------------------------------
// Arguments / Directives
// ---------------------------------------------------------------------------

fn parse_arguments(
  parser: Parser,
  is_const: Bool,
) -> Result(#(List(Argument), Parser), ParseError) {
  optional_many(
    parser,
    tk.ParenL,
    fn(p) { parse_argument(p, is_const) },
    tk.ParenR,
  )
}

fn parse_argument(
  parser: Parser,
  is_const: Bool,
) -> Result(#(Argument, Parser), ParseError) {
  let start = parser.token
  use #(name, parser) <- result.try(parse_name(parser))
  use #(_, parser) <- result.try(expect_token(parser, tk.Colon))
  use #(value, parser) <- result.try(parse_value_literal(parser, is_const))
  let loc = Some(Location(start: start.start, end: parser.last_token.end))
  Ok(#(Argument(name: name, value: value, loc: loc), parser))
}

fn parse_directives(
  parser: Parser,
  is_const: Bool,
) -> Result(#(List(Directive), Parser), ParseError) {
  parse_directives_loop(parser, is_const, [])
}

fn parse_directives_loop(
  parser: Parser,
  is_const: Bool,
  acc: List(Directive),
) -> Result(#(List(Directive), Parser), ParseError) {
  case peek(parser, tk.At) {
    True -> {
      use #(d, parser) <- result.try(parse_directive(parser, is_const))
      parse_directives_loop(parser, is_const, [d, ..acc])
    }
    False -> Ok(#(list.reverse(acc), parser))
  }
}

fn parse_directive(
  parser: Parser,
  is_const: Bool,
) -> Result(#(Directive, Parser), ParseError) {
  let start = parser.token
  use #(_, parser) <- result.try(expect_token(parser, tk.At))
  use #(name, parser) <- result.try(parse_name(parser))
  use #(args, parser) <- result.try(parse_arguments(parser, is_const))
  let loc = Some(Location(start: start.start, end: parser.last_token.end))
  Ok(#(Directive(name: name, arguments: args, loc: loc), parser))
}

// ---------------------------------------------------------------------------
// Value literals
// ---------------------------------------------------------------------------

fn parse_value_literal(
  parser: Parser,
  is_const: Bool,
) -> Result(#(Value, Parser), ParseError) {
  let token_now = parser.token
  case token_now.kind {
    tk.BracketL -> parse_list(parser, is_const)
    tk.BraceL -> parse_object(parser, is_const)
    tk.Int -> {
      use parser <- result.try(advance_parser(parser))
      let v = case token_now.value {
        Some(v) -> v
        None -> ""
      }
      let loc = Some(Location(start: token_now.start, end: token_now.end))
      Ok(#(IntValue(value: v, loc: loc), parser))
    }
    tk.Float -> {
      use parser <- result.try(advance_parser(parser))
      let v = case token_now.value {
        Some(v) -> v
        None -> ""
      }
      let loc = Some(Location(start: token_now.start, end: token_now.end))
      Ok(#(FloatValue(value: v, loc: loc), parser))
    }
    tk.String -> {
      use parser <- result.try(advance_parser(parser))
      let v = case token_now.value {
        Some(v) -> v
        None -> ""
      }
      let loc = Some(Location(start: token_now.start, end: token_now.end))
      Ok(#(StringValue(value: v, block: False, loc: loc), parser))
    }
    tk.Name -> {
      use parser <- result.try(advance_parser(parser))
      let loc = Some(Location(start: token_now.start, end: token_now.end))
      case token_now.value {
        Some("true") -> Ok(#(BooleanValue(value: True, loc: loc), parser))
        Some("false") -> Ok(#(BooleanValue(value: False, loc: loc), parser))
        Some("null") -> Ok(#(NullValue(loc: loc), parser))
        Some(name) -> Ok(#(EnumValue(value: name, loc: loc), parser))
        None -> Error(unexpected_token(token_now))
      }
    }
    tk.Dollar ->
      case is_const {
        True ->
          Error(token_error_at(
            token_now,
            "Unexpected variable in constant value.",
          ))
        False -> {
          use #(v, parser) <- result.try(parse_variable(parser))
          Ok(#(VariableValue(variable: v), parser))
        }
      }
    _ -> Error(unexpected(parser))
  }
}

fn parse_list(
  parser: Parser,
  is_const: Bool,
) -> Result(#(Value, Parser), ParseError) {
  let start = parser.token
  use #(values, parser) <- result.try(any_(
    parser,
    tk.BracketL,
    fn(p) { parse_value_literal(p, is_const) },
    tk.BracketR,
  ))
  let loc = Some(Location(start: start.start, end: parser.last_token.end))
  Ok(#(ListValue(values: values, loc: loc), parser))
}

fn parse_object(
  parser: Parser,
  is_const: Bool,
) -> Result(#(Value, Parser), ParseError) {
  let start = parser.token
  use #(fields, parser) <- result.try(any_(
    parser,
    tk.BraceL,
    fn(p) { parse_object_field(p, is_const) },
    tk.BraceR,
  ))
  let loc = Some(Location(start: start.start, end: parser.last_token.end))
  Ok(#(ObjectValue(fields: fields, loc: loc), parser))
}

fn parse_object_field(
  parser: Parser,
  is_const: Bool,
) -> Result(#(ObjectField, Parser), ParseError) {
  let start = parser.token
  use #(name, parser) <- result.try(parse_name(parser))
  use #(_, parser) <- result.try(expect_token(parser, tk.Colon))
  use #(value, parser) <- result.try(parse_value_literal(parser, is_const))
  let loc = Some(Location(start: start.start, end: parser.last_token.end))
  Ok(#(ObjectField(name: name, value: value, loc: loc), parser))
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

fn parse_type_reference(
  parser: Parser,
) -> Result(#(TypeRef, Parser), ParseError) {
  let start = parser.token
  use #(was_bracket, parser) <- result.try(expect_optional_token(
    parser,
    tk.BracketL,
  ))
  use #(inner, parser) <- result.try(case was_bracket {
    True -> {
      use #(inner, parser) <- result.try(parse_type_reference(parser))
      use #(_, parser) <- result.try(expect_token(parser, tk.BracketR))
      let loc = Some(Location(start: start.start, end: parser.last_token.end))
      Ok(#(ListType(inner: inner, loc: loc), parser))
    }
    False -> parse_named_type(parser)
  })
  use #(was_bang, parser) <- result.try(expect_optional_token(parser, tk.Bang))
  case was_bang {
    True -> {
      let loc = Some(Location(start: start.start, end: parser.last_token.end))
      Ok(#(NonNullType(inner: inner, loc: loc), parser))
    }
    False -> Ok(#(inner, parser))
  }
}

fn parse_named_type(parser: Parser) -> Result(#(TypeRef, Parser), ParseError) {
  let start = parser.token
  use #(name, parser) <- result.try(parse_name(parser))
  let loc = Some(Location(start: start.start, end: parser.last_token.end))
  Ok(#(NamedType(name: name, loc: loc), parser))
}

// ---------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------

fn parse_name(parser: Parser) -> Result(#(Name, Parser), ParseError) {
  let saved = parser.token
  use #(t, parser) <- result.try(expect_token(parser, tk.Name))
  let value = case t.value {
    Some(v) -> v
    None -> ""
  }
  let loc = Some(Location(start: saved.start, end: saved.end))
  Ok(#(Name(value: value, loc: loc), parser))
}

// ---------------------------------------------------------------------------
// Token-level helpers (peek / expect / advance)
// ---------------------------------------------------------------------------

fn peek(parser: Parser, kind: tk.TokenKind) -> Bool {
  parser.token.kind == kind
}

fn advance_parser(parser: Parser) -> Result(Parser, ParseError) {
  case lexer.next_token(parser.lexer) {
    Error(e) -> Error(lex_to_parse(e))
    Ok(#(next, after)) ->
      Ok(Parser(lexer: after, token: next, last_token: parser.token))
  }
}

fn expect_token(
  parser: Parser,
  kind: tk.TokenKind,
) -> Result(#(Token, Parser), ParseError) {
  case parser.token.kind == kind {
    True -> {
      let consumed = parser.token
      use parser <- result.map(advance_parser(parser))
      #(consumed, parser)
    }
    False ->
      Error(token_error(
        parser,
        "Expected "
          <> tk.display(kind)
          <> ", found "
          <> token_desc(parser.token)
          <> ".",
      ))
  }
}

fn expect_optional_token(
  parser: Parser,
  kind: tk.TokenKind,
) -> Result(#(Bool, Parser), ParseError) {
  case parser.token.kind == kind {
    True -> {
      use parser <- result.map(advance_parser(parser))
      #(True, parser)
    }
    False -> Ok(#(False, parser))
  }
}

fn expect_keyword(parser: Parser, value: String) -> Result(Parser, ParseError) {
  case parser.token.kind == tk.Name && parser.token.value == Some(value) {
    True -> advance_parser(parser)
    False ->
      Error(token_error(
        parser,
        "Expected \""
          <> value
          <> "\", found "
          <> token_desc(parser.token)
          <> ".",
      ))
  }
}

fn expect_optional_keyword(
  parser: Parser,
  value: String,
) -> Result(#(Bool, Parser), ParseError) {
  case parser.token.kind == tk.Name && parser.token.value == Some(value) {
    True -> {
      use parser <- result.map(advance_parser(parser))
      #(True, parser)
    }
    False -> Ok(#(False, parser))
  }
}

// ---------------------------------------------------------------------------
// List helpers (`many`, `optionalMany`, `any`)
// ---------------------------------------------------------------------------

fn many(
  parser: Parser,
  open: tk.TokenKind,
  parse_fn: fn(Parser) -> Result(#(a, Parser), ParseError),
  close: tk.TokenKind,
) -> Result(#(List(a), Parser), ParseError) {
  use #(_, parser) <- result.try(expect_token(parser, open))
  many_loop(parser, parse_fn, close, [])
}

fn many_loop(
  parser: Parser,
  parse_fn: fn(Parser) -> Result(#(a, Parser), ParseError),
  close: tk.TokenKind,
  acc: List(a),
) -> Result(#(List(a), Parser), ParseError) {
  use #(item, parser) <- result.try(parse_fn(parser))
  let acc = [item, ..acc]
  use #(closed, parser) <- result.try(expect_optional_token(parser, close))
  case closed {
    True -> Ok(#(list.reverse(acc), parser))
    False -> many_loop(parser, parse_fn, close, acc)
  }
}

fn optional_many(
  parser: Parser,
  open: tk.TokenKind,
  parse_fn: fn(Parser) -> Result(#(a, Parser), ParseError),
  close: tk.TokenKind,
) -> Result(#(List(a), Parser), ParseError) {
  use #(opened, parser) <- result.try(expect_optional_token(parser, open))
  case opened {
    True -> many_loop(parser, parse_fn, close, [])
    False -> Ok(#([], parser))
  }
}

fn any_(
  parser: Parser,
  open: tk.TokenKind,
  parse_fn: fn(Parser) -> Result(#(a, Parser), ParseError),
  close: tk.TokenKind,
) -> Result(#(List(a), Parser), ParseError) {
  use #(_, parser) <- result.try(expect_token(parser, open))
  any_loop(parser, parse_fn, close, [])
}

fn any_loop(
  parser: Parser,
  parse_fn: fn(Parser) -> Result(#(a, Parser), ParseError),
  close: tk.TokenKind,
  acc: List(a),
) -> Result(#(List(a), Parser), ParseError) {
  use #(closed, parser) <- result.try(expect_optional_token(parser, close))
  case closed {
    True -> Ok(#(list.reverse(acc), parser))
    False -> {
      use #(item, parser) <- result.try(parse_fn(parser))
      any_loop(parser, parse_fn, close, [item, ..acc])
    }
  }
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

fn unexpected(parser: Parser) -> ParseError {
  token_error(parser, "Unexpected " <> token_desc(parser.token) <> ".")
}

fn unexpected_token(t: Token) -> ParseError {
  token_error_at(t, "Unexpected " <> token_desc(t) <> ".")
}

fn token_desc(t: Token) -> String {
  let kind_desc = case tk.is_punctuator(t.kind) {
    True -> "\"" <> tk.display(t.kind) <> "\""
    False -> tk.display(t.kind)
  }
  case t.value {
    Some(v) -> kind_desc <> " \"" <> v <> "\""
    None -> kind_desc
  }
}

fn token_error(parser: Parser, message: String) -> ParseError {
  ParseError(
    message: message,
    position: parser.token.start,
    line: parser.token.line,
    column: parser.token.column,
  )
}

fn token_error_at(t: Token, message: String) -> ParseError {
  ParseError(
    message: message,
    position: t.start,
    line: t.line,
    column: t.column,
  )
}

fn lex_to_parse(e: lexer.LexError) -> ParseError {
  ParseError(
    message: e.message,
    position: e.position,
    line: e.line,
    column: e.column,
  )
}
