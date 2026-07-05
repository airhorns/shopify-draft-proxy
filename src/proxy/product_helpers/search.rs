use super::*;

pub(in crate::proxy) fn product_matches_search_query(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    query: &str,
) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let tokens = product_search_tokens(query);
    if tokens.is_empty() {
        return true;
    }
    let mut parser = ProductSearchParser::new(tokens);
    parser
        .parse()
        .map(|expression| expression.matches(product, variants))
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProductSearchToken {
    Term { value: String, quoted: bool },
    LParen,
    RParen,
    Minus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProductSearchExpression {
    Term(ProductSearchTerm),
    Not(Box<ProductSearchExpression>),
    And(Vec<ProductSearchExpression>),
    Or(Vec<ProductSearchExpression>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProductSearchTerm {
    field: Option<String>,
    value: String,
}

struct ProductSearchParser {
    tokens: Vec<ProductSearchToken>,
    index: usize,
}

impl ProductSearchParser {
    fn new(tokens: Vec<ProductSearchToken>) -> Self {
        Self { tokens, index: 0 }
    }

    fn parse(&mut self) -> Option<ProductSearchExpression> {
        let expression = self.parse_or()?;
        Some(expression)
    }

    fn parse_or(&mut self) -> Option<ProductSearchExpression> {
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
            ProductSearchExpression::Or(expressions)
        })
    }

    fn parse_and(&mut self) -> Option<ProductSearchExpression> {
        let mut expressions = Vec::new();
        while self.index < self.tokens.len() {
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
            ProductSearchExpression::And(expressions)
        })
    }

    fn parse_unary(&mut self) -> Option<ProductSearchExpression> {
        if matches!(self.tokens.get(self.index), Some(ProductSearchToken::Minus)) {
            self.index += 1;
            return self
                .parse_unary()
                .map(|expression| ProductSearchExpression::Not(Box::new(expression)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Option<ProductSearchExpression> {
        match self.tokens.get(self.index).cloned()? {
            ProductSearchToken::Term { value, quoted } => {
                self.index += 1;
                Some(ProductSearchExpression::Term(ProductSearchTerm::new(
                    value, quoted,
                )))
            }
            ProductSearchToken::LParen => {
                self.index += 1;
                let expression = self.parse_or()?;
                if self.peek_rparen() {
                    self.index += 1;
                }
                Some(expression)
            }
            ProductSearchToken::RParen | ProductSearchToken::Minus => None,
        }
    }

    fn peek_rparen(&self) -> bool {
        matches!(
            self.tokens.get(self.index),
            Some(ProductSearchToken::RParen)
        )
    }

    fn peek_operator(&self, operator: &str) -> bool {
        matches!(
            self.tokens.get(self.index),
            Some(ProductSearchToken::Term { value, quoted: false })
                if value.eq_ignore_ascii_case(operator)
        )
    }

    fn consume_operator(&mut self, operator: &str) -> bool {
        if self.peek_operator(operator) {
            self.index += 1;
            true
        } else {
            false
        }
    }
}

impl ProductSearchExpression {
    fn matches(&self, product: &ProductRecord, variants: &[ProductVariantRecord]) -> bool {
        match self {
            ProductSearchExpression::Term(term) => term.matches(product, variants),
            ProductSearchExpression::Not(expression) => !expression.matches(product, variants),
            ProductSearchExpression::And(expressions) => expressions
                .iter()
                .all(|expression| expression.matches(product, variants)),
            ProductSearchExpression::Or(expressions) => expressions
                .iter()
                .any(|expression| expression.matches(product, variants)),
        }
    }
}

impl ProductSearchTerm {
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

    fn matches(&self, product: &ProductRecord, variants: &[ProductVariantRecord]) -> bool {
        let value = self.value.trim();
        if value.is_empty() {
            return true;
        }
        match self.field.as_deref() {
            Some("id") => product_matches_search_id(product, value),
            Some("status") => product.status.eq_ignore_ascii_case(value),
            Some("vendor") => product_search_string_matches(&product.vendor, value),
            Some("product_type") => product_search_string_matches(&product.product_type, value),
            Some("title") => product_search_string_matches(&product.title, value),
            Some("handle") => product_search_string_matches(&product.handle, value),
            Some("tag") => product_matches_search_tag(product, value),
            Some("tag_not") => !product_matches_search_tag(product, value),
            Some("sku") => product_matches_search_sku(product, variants, value),
            Some("barcode") => product_matches_search_barcode(product, variants, value),
            Some("gift_card") => product_matches_search_gift_card(product, value),
            Some("collection_id") => product_matches_search_collection_id(product, value),
            Some("published_status") => product_matches_published_status(product, value),
            Some("published_at") => product_matches_published_at(product, value),
            Some("created_at") => product_matches_date_query(&product.created_at, value),
            Some("updated_at") => product_matches_date_query(&product.updated_at, value),
            Some(_) => false,
            None => product_matches_free_text(product, variants, value),
        }
    }
}

fn product_search_tokens(query: &str) -> Vec<ProductSearchToken> {
    let mut tokens = Vec::new();
    let chars = query.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            ch if ch.is_whitespace() => {
                index += 1;
            }
            '(' => {
                tokens.push(ProductSearchToken::LParen);
                index += 1;
            }
            ')' => {
                tokens.push(ProductSearchToken::RParen);
                index += 1;
            }
            '-' => {
                tokens.push(ProductSearchToken::Minus);
                index += 1;
            }
            '"' | '\'' => {
                let quote = chars[index];
                index += 1;
                let mut value = String::new();
                while index < chars.len() && chars[index] != quote {
                    value.push(chars[index]);
                    index += 1;
                }
                if index < chars.len() {
                    index += 1;
                }
                tokens.push(ProductSearchToken::Term {
                    value,
                    quoted: true,
                });
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
                        index += 1;
                        while index < chars.len() && chars[index] != quote {
                            value.push(chars[index]);
                            index += 1;
                        }
                        if index < chars.len() {
                            index += 1;
                        }
                    } else {
                        value.push(chars[index]);
                        index += 1;
                    }
                }
                if !value.is_empty() {
                    tokens.push(ProductSearchToken::Term {
                        value,
                        quoted: false,
                    });
                }
            }
        }
    }
    tokens
}

fn product_matches_free_text(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    value: &str,
) -> bool {
    product_search_string_matches(&product.title, value)
        || product_search_string_matches(&product.handle, value)
        || product_search_string_matches(&product.vendor, value)
        || product_search_string_matches(&product.product_type, value)
        || product_matches_search_tag(product, value)
        || product_matches_search_sku(product, variants, value)
}

fn product_matches_search_id(product: &ProductRecord, value: &str) -> bool {
    let value = value.trim_matches('"').trim_matches('\'');
    product.id == value || resource_id_path_tail(&product.id) == value
}

fn product_matches_search_tag(product: &ProductRecord, value: &str) -> bool {
    product
        .tags
        .iter()
        .any(|tag| product_search_string_matches(tag, value))
}

fn product_matches_search_sku(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    value: &str,
) -> bool {
    variants
        .iter()
        .any(|variant| product_search_string_matches(&variant.sku, value))
        || product.variants.iter().any(|variant| {
            variant
                .get("sku")
                .and_then(Value::as_str)
                .is_some_and(|sku| product_search_string_matches(sku, value))
        })
}

fn product_matches_search_barcode(
    product: &ProductRecord,
    variants: &[ProductVariantRecord],
    value: &str,
) -> bool {
    variants.iter().any(|variant| {
        variant
            .barcode
            .as_deref()
            .is_some_and(|barcode| product_search_string_matches(barcode, value))
    }) || product.variants.iter().any(|variant| {
        variant
            .get("barcode")
            .and_then(Value::as_str)
            .is_some_and(|barcode| product_search_string_matches(barcode, value))
    })
}

fn product_matches_search_gift_card(product: &ProductRecord, value: &str) -> bool {
    let actual = product
        .extra_fields
        .get("isGiftCard")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match value.to_ascii_lowercase().as_str() {
        "true" => actual,
        "false" => !actual,
        _ => false,
    }
}

fn product_matches_search_collection_id(product: &ProductRecord, value: &str) -> bool {
    let value = value.trim_matches('"').trim_matches('\'');
    product.collections.iter().any(|collection| {
        collection
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id == value || resource_id_path_tail(id) == value)
    })
}

pub(in crate::proxy) fn product_search_string_matches(actual: &str, query_value: &str) -> bool {
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

fn product_matches_published_status(product: &ProductRecord, value: &str) -> bool {
    let published = product_is_published(product);
    match value.to_ascii_lowercase().as_str() {
        "published" => published,
        "unpublished" => !published,
        "any" => true,
        _ => false,
    }
}

fn product_matches_published_at(product: &ProductRecord, value: &str) -> bool {
    product
        .extra_fields
        .get("publishedAt")
        .and_then(Value::as_str)
        .is_some_and(|published_at| product_matches_date_query(published_at, value))
}

fn product_is_published(product: &ProductRecord) -> bool {
    product
        .extra_fields
        .get("publishedAt")
        .is_some_and(|published_at| !published_at.is_null())
        || !product_visible_publication_entries(product).is_empty()
}

pub(in crate::proxy) fn product_matches_date_query(actual: &str, query_value: &str) -> bool {
    let (operator, expected) = product_search_comparator(query_value);
    match operator {
        "<" => actual < expected,
        "<=" => actual <= expected,
        ">" => actual > expected,
        ">=" => actual >= expected,
        _ => actual.starts_with(expected),
    }
}

fn product_search_comparator(value: &str) -> (&str, &str) {
    for operator in [">=", "<=", ">", "<", "="] {
        if let Some(rest) = value.strip_prefix(operator) {
            return (operator, rest);
        }
    }
    ("=", value)
}
