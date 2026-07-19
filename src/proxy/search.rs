use super::connection::StagedSearchDecision;

pub(in crate::proxy) enum ParsedSearchExpression {
    Term(ParsedSearchTerm),
    Not(Box<ParsedSearchExpression>),
    And(Vec<ParsedSearchExpression>),
    Or(Vec<ParsedSearchExpression>),
}

pub(in crate::proxy) struct ParsedSearchTerm {
    pub(in crate::proxy) field: Option<String>,
    pub(in crate::proxy) value: String,
}

#[derive(Clone)]
enum SearchToken {
    Term(String, bool),
    LParen,
    RParen,
    Minus,
}

struct SearchParser(Vec<SearchToken>, usize);

impl ParsedSearchExpression {
    pub(in crate::proxy) fn matches_with<F>(&self, matches_term: &mut F) -> bool
    where
        F: FnMut(&ParsedSearchTerm) -> bool,
    {
        match self {
            ParsedSearchExpression::Term(term) => matches_term(term),
            ParsedSearchExpression::Not(expression) => !expression.matches_with(matches_term),
            ParsedSearchExpression::And(expressions) => expressions
                .iter()
                .all(|expression| expression.matches_with(matches_term)),
            ParsedSearchExpression::Or(expressions) => expressions
                .iter()
                .any(|expression| expression.matches_with(matches_term)),
        }
    }

    fn for_each_term<F>(&self, visit: &mut F)
    where
        F: FnMut(&ParsedSearchTerm),
    {
        match self {
            ParsedSearchExpression::Term(term) => visit(term),
            ParsedSearchExpression::Not(expression) => expression.for_each_term(visit),
            ParsedSearchExpression::And(expressions) | ParsedSearchExpression::Or(expressions) => {
                for expression in expressions {
                    expression.for_each_term(visit);
                }
            }
        }
    }
}

impl ParsedSearchTerm {
    fn new(value: String, quoted: bool) -> Self {
        if !quoted {
            if let Some((field, value)) = value.split_once(':') {
                if !field.is_empty() && !value.is_empty() {
                    return Self {
                        field: Some(field.to_ascii_lowercase()),
                        value: value.trim_matches('"').trim_matches('\'').to_string(),
                    };
                }
            }
        }
        Self { field: None, value }
    }
}

impl SearchParser {
    fn parse_or(&mut self) -> Option<ParsedSearchExpression> {
        let mut expressions = vec![self.parse_and()?];
        while self.consume_operator("OR") {
            let Some(right) = self.parse_and() else {
                break;
            };
            expressions.push(right);
        }
        Some(if expressions.len() == 1 {
            expressions.remove(0)
        } else {
            ParsedSearchExpression::Or(expressions)
        })
    }

    fn parse_and(&mut self) -> Option<ParsedSearchExpression> {
        let mut expressions = Vec::new();
        while self.1 < self.0.len() {
            if self.peek_rparen() || self.peek_operator("OR") {
                break;
            }
            self.consume_operator("AND");
            if self.peek_rparen() || self.peek_operator("OR") {
                break;
            }
            if let Some(expression) = self.parse_unary() {
                expressions.push(expression);
            } else {
                break;
            }
        }
        Some(if expressions.len() == 1 {
            expressions.remove(0)
        } else {
            ParsedSearchExpression::And(expressions)
        })
    }

    fn parse_unary(&mut self) -> Option<ParsedSearchExpression> {
        if matches!(self.0.get(self.1), Some(SearchToken::Minus)) {
            self.1 += 1;
            return self
                .parse_unary()
                .map(|expression| ParsedSearchExpression::Not(Box::new(expression)));
        }
        if self.consume_operator("NOT") {
            return self
                .parse_unary()
                .map(|expression| ParsedSearchExpression::Not(Box::new(expression)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<ParsedSearchExpression> {
        match self.0.get(self.1).cloned()? {
            SearchToken::Term(value, quoted) => {
                self.1 += 1;
                Some(ParsedSearchExpression::Term(ParsedSearchTerm::new(
                    value, quoted,
                )))
            }
            SearchToken::LParen => {
                self.1 += 1;
                let expression = self.parse_or()?;
                if self.peek_rparen() {
                    self.1 += 1;
                }
                Some(expression)
            }
            SearchToken::RParen | SearchToken::Minus => None,
        }
    }

    fn peek_rparen(&self) -> bool {
        matches!(self.0.get(self.1), Some(SearchToken::RParen))
    }

    fn peek_operator(&self, operator: &str) -> bool {
        matches!(
            self.0.get(self.1),
            Some(SearchToken::Term(value, false)) if value.eq_ignore_ascii_case(operator)
        )
    }

    fn consume_operator(&mut self, operator: &str) -> bool {
        if self.peek_operator(operator) {
            self.1 += 1;
            true
        } else {
            false
        }
    }
}

pub(in crate::proxy) fn parse_search_query(query: &str) -> Option<ParsedSearchExpression> {
    let tokens = search_query_tokens(query.trim());
    (!tokens.is_empty())
        .then(|| SearchParser(tokens, 0).parse_or())
        .flatten()
}

pub(in crate::proxy) fn search_query_decision<F>(
    query: Option<&str>,
    mut matches_term: F,
) -> StagedSearchDecision
where
    F: FnMut(&ParsedSearchTerm) -> StagedSearchDecision,
{
    let Some(query) = query.map(str::trim).filter(|query| !query.is_empty()) else {
        return StagedSearchDecision::Match;
    };
    let Some(expression) = parse_search_query(query) else {
        return StagedSearchDecision::Unsupported;
    };

    let mut saw_unsupported = false;
    expression.for_each_term(&mut |term| {
        if matches!(matches_term(term), StagedSearchDecision::Unsupported) {
            saw_unsupported = true;
        }
    });
    if saw_unsupported {
        return StagedSearchDecision::Unsupported;
    }

    StagedSearchDecision::from_bool(
        expression
            .matches_with(&mut |term| matches!(matches_term(term), StagedSearchDecision::Match)),
    )
}

fn search_query_tokens(query: &str) -> Vec<SearchToken> {
    let mut tokens = Vec::new();
    let chars = query.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            ch if ch.is_whitespace() => {
                index += 1;
            }
            '(' => {
                tokens.push(SearchToken::LParen);
                index += 1;
            }
            ')' => {
                tokens.push(SearchToken::RParen);
                index += 1;
            }
            '-' => {
                tokens.push(SearchToken::Minus);
                index += 1;
            }
            '"' | '\'' => {
                let quote = chars[index];
                tokens.push(SearchToken::Term(
                    quoted_search_value(&chars, &mut index, quote),
                    true,
                ));
            }
            _ => {
                let mut value = String::new();
                while index < chars.len()
                    && !chars[index].is_whitespace()
                    && chars[index] != '('
                    && chars[index] != ')'
                {
                    if chars[index] == '"' || chars[index] == '\'' {
                        let quote = chars[index];
                        value.push_str(&quoted_search_value(&chars, &mut index, quote));
                    } else {
                        value.push(chars[index]);
                        index += 1;
                    }
                }
                if !value.is_empty() {
                    tokens.push(SearchToken::Term(value, false));
                }
            }
        }
    }
    tokens
}

fn quoted_search_value(chars: &[char], index: &mut usize, quote: char) -> String {
    *index += 1;
    let mut value = String::new();
    while *index < chars.len() && chars[*index] != quote {
        value.push(chars[*index]);
        *index += 1;
    }
    if *index < chars.len() {
        *index += 1;
    }
    value
}

pub(in crate::proxy) fn split_search_query_terms(query: &str, quote: char) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    for ch in query.chars() {
        if ch == quote {
            in_quotes = !in_quotes;
            current.push(ch);
        } else if ch.is_whitespace() && !in_quotes {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub(in crate::proxy) fn search_string_matches(actual: &str, query_value: &str) -> bool {
    let actual = actual.to_ascii_lowercase();
    let query_value = query_value
        .trim_matches('"')
        .trim_matches('\'')
        .to_ascii_lowercase();
    if query_value.is_empty() {
        return true;
    }
    if let Some(prefix) = query_value.strip_suffix('*') {
        return actual
            .split(|ch: char| !ch.is_ascii_alphanumeric())
            .any(|part| part.starts_with(prefix));
    }
    actual.contains(&query_value)
}

pub(in crate::proxy) fn search_comparator(value: &str) -> (&str, &str) {
    for operator in [">=", "<=", ">", "<", "="] {
        if let Some(rest) = value.strip_prefix(operator) {
            return (operator, rest);
        }
    }
    ("=", value)
}
