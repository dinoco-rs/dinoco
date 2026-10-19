//! The condition of a partial index (`@unique<email != null>`), as migrations
//! carry it from the schema to each database and read it back.
//!
//! Every database stores the condition in Dinoco's own syntax next to the
//! index (a SQL comment in SQLite's index definition, `COMMENT ON INDEX` on
//! PostgreSQL, the index `COMMENT` on MySQL), so drift detection compares
//! conditions exactly instead of parsing each database's rendering of them.

use std::fmt;

/// Prefix of the comment that stores a condition next to its index.
pub const INDEX_PREDICATE_COMMENT: &str = "dinoco:where ";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum IndexPredicate {
    /// `field <operator> value`.
    Compare {
        field: String,
        operator: PredicateOperator,
        value: PredicateValue,
    },
    /// A Boolean column that is `true`.
    Field(String),
    Not(Box<IndexPredicate>),
    And(Vec<IndexPredicate>),
    Or(Vec<IndexPredicate>),
    /// A condition Dinoco did not write, read back as the database's SQL. It
    /// never equals a schema condition, so the index is recreated.
    Raw(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PredicateOperator {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PredicateValue {
    Null,
    Boolean(bool),
    /// The number as written in the schema.
    Number(String),
    /// A string, date or enum value.
    String(String),
}

impl PredicateOperator {
    fn schema(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::Neq => "!=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Gt => ">",
            Self::Gte => ">=",
        }
    }

    fn sql(self) -> &'static str {
        match self {
            Self::Eq => " = ",
            Self::Neq => " <> ",
            Self::Lt => " < ",
            Self::Lte => " <= ",
            Self::Gt => " > ",
            Self::Gte => " >= ",
        }
    }
}

impl IndexPredicate {
    /// Every field the condition reads, in order of appearance.
    pub fn fields(&self) -> Vec<&str> {
        fn collect<'a>(predicate: &'a IndexPredicate, fields: &mut Vec<&'a str>) {
            match predicate {
                IndexPredicate::Compare { field, .. } | IndexPredicate::Field(field) => fields.push(field),
                IndexPredicate::Not(predicate) => collect(predicate, fields),
                IndexPredicate::And(predicates) | IndexPredicate::Or(predicates) => {
                    predicates.iter().for_each(|predicate| collect(predicate, fields))
                }
                IndexPredicate::Raw(_) => {}
            }
        }

        let mut fields = Vec::new();
        collect(self, &mut fields);
        fields
    }

    /// The comment stored next to the index: [`INDEX_PREDICATE_COMMENT`]
    /// followed by the condition in Dinoco's syntax.
    pub fn comment(&self) -> String {
        format!("{INDEX_PREDICATE_COMMENT}{self}")
    }

    /// The condition of a comment written by [`comment`](Self::comment).
    pub fn from_comment(comment: &str) -> Option<Self> {
        Self::parse(comment.trim().strip_prefix(INDEX_PREDICATE_COMMENT)?).ok()
    }

    /// Parses a condition in Dinoco's syntax, as [`Display`](fmt::Display)
    /// writes it.
    pub fn parse(source: &str) -> Result<Self, String> {
        let mut parser = Parser { source, offset: 0 };
        let predicate = parser.or()?;
        parser.skip_whitespace();
        if parser.offset != source.len() {
            return Err(format!("unexpected `{}` in index condition", &source[parser.offset..]));
        }
        Ok(predicate)
    }

    /// The condition as SQL, with each field written by `column`. Strings are
    /// single-quoted, with backslashes escaped too when `backslash_escapes`
    /// (MySQL's default `sql_mode`).
    pub fn to_sql(&self, column: &dyn Fn(&str) -> String, backslash_escapes: bool) -> String {
        let mut sql = String::new();
        self.write_sql(&mut sql, column, backslash_escapes, false);
        sql
    }

    fn write_sql(&self, sql: &mut String, column: &dyn Fn(&str) -> String, backslash_escapes: bool, nested: bool) {
        match self {
            Self::Compare { field, operator, value: PredicateValue::Null } => {
                sql.push_str(&column(field));
                match operator {
                    PredicateOperator::Eq => sql.push_str(" IS NULL"),
                    PredicateOperator::Neq => sql.push_str(" IS NOT NULL"),
                    operator => {
                        sql.push_str(operator.sql());
                        sql.push_str("NULL");
                    }
                }
            }
            Self::Compare { field, operator, value } => {
                sql.push_str(&column(field));
                sql.push_str(operator.sql());
                match value {
                    PredicateValue::Null => sql.push_str("NULL"),
                    PredicateValue::Boolean(value) => sql.push_str(if *value { "TRUE" } else { "FALSE" }),
                    PredicateValue::Number(value) => sql.push_str(value),
                    PredicateValue::String(value) => write_sql_string(sql, value, backslash_escapes),
                }
            }
            Self::Field(field) => {
                sql.push_str(&column(field));
                sql.push_str(" = TRUE");
            }
            Self::Not(predicate) => {
                sql.push_str("NOT (");
                predicate.write_sql(sql, column, backslash_escapes, false);
                sql.push(')');
            }
            Self::And(predicates) | Self::Or(predicates) => {
                let separator = if matches!(self, Self::And(_)) { " AND " } else { " OR " };
                if nested {
                    sql.push('(');
                }
                for (index, predicate) in predicates.iter().enumerate() {
                    if index > 0 {
                        sql.push_str(separator);
                    }
                    predicate.write_sql(sql, column, backslash_escapes, true);
                }
                if nested {
                    sql.push(')');
                }
            }
            Self::Raw(raw) => {
                if nested {
                    sql.push('(');
                }
                sql.push_str(raw);
                if nested {
                    sql.push(')');
                }
            }
        }
    }

    fn write(&self, out: &mut fmt::Formatter<'_>, parent: u8) -> fmt::Result {
        // Precedence: `||` 1, `&&` 2, everything else binds tighter.
        let (own, separator, predicates) = match self {
            Self::Or(predicates) => (1, " || ", predicates),
            Self::And(predicates) => (2, " && ", predicates),
            Self::Not(predicate) => {
                out.write_str("!")?;
                return match predicate.as_ref() {
                    Self::Field(_) | Self::Not(_) => predicate.write(out, 3),
                    predicate => {
                        out.write_str("(")?;
                        predicate.write(out, 0)?;
                        out.write_str(")")
                    }
                };
            }
            Self::Field(field) => return out.write_str(field),
            Self::Raw(raw) => return write!(out, "sql({raw})"),
            Self::Compare { field, operator, value } => {
                write!(out, "{field} {} ", operator.schema())?;
                return match value {
                    PredicateValue::Null => out.write_str("null"),
                    PredicateValue::Boolean(value) => write!(out, "{value}"),
                    PredicateValue::Number(value) => out.write_str(value),
                    PredicateValue::String(value) => {
                        out.write_str("\"")?;
                        for character in value.chars() {
                            match character {
                                '"' => out.write_str("\\\"")?,
                                '\\' => out.write_str("\\\\")?,
                                '\n' => out.write_str("\\n")?,
                                '\r' => out.write_str("\\r")?,
                                '\t' => out.write_str("\\t")?,
                                // Keeps `*/` out of SQLite's comment.
                                '/' => out.write_str("\\/")?,
                                character => write!(out, "{character}")?,
                            }
                        }
                        out.write_str("\"")
                    }
                };
            }
        };

        let grouped = own < parent;
        if grouped {
            out.write_str("(")?;
        }
        for (index, predicate) in predicates.iter().enumerate() {
            if index > 0 {
                out.write_str(separator)?;
            }
            predicate.write(out, own + 1)?;
        }
        if grouped {
            out.write_str(")")?;
        }
        Ok(())
    }
}

/// The condition in Dinoco's schema syntax: `a != null && (b == "x" || !c)`.
impl fmt::Display for IndexPredicate {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write(out, 0)
    }
}

fn write_sql_string(sql: &mut String, value: &str, backslash_escapes: bool) {
    sql.push('\'');
    for character in value.chars() {
        match character {
            '\'' => sql.push_str("''"),
            '\\' if backslash_escapes => sql.push_str("\\\\"),
            character => sql.push(character),
        }
    }
    sql.push('\'');
}

struct Parser<'a> {
    source: &'a str,
    offset: usize,
}

impl Parser<'_> {
    fn rest(&self) -> &str {
        &self.source[self.offset..]
    }

    fn skip_whitespace(&mut self) {
        let trimmed = self.rest().trim_start();
        self.offset = self.source.len() - trimmed.len();
    }

    fn eat(&mut self, token: &str) -> bool {
        self.skip_whitespace();
        if self.rest().starts_with(token) {
            self.offset += token.len();
            true
        } else {
            false
        }
    }

    fn or(&mut self) -> Result<IndexPredicate, String> {
        let mut predicates = vec![self.and()?];
        while self.eat("||") {
            predicates.push(self.and()?);
        }
        Ok(if predicates.len() == 1 { predicates.remove(0) } else { IndexPredicate::Or(predicates) })
    }

    fn and(&mut self) -> Result<IndexPredicate, String> {
        let mut predicates = vec![self.unary()?];
        while self.eat("&&") {
            predicates.push(self.unary()?);
        }
        Ok(if predicates.len() == 1 { predicates.remove(0) } else { IndexPredicate::And(predicates) })
    }

    fn unary(&mut self) -> Result<IndexPredicate, String> {
        self.skip_whitespace();
        if self.rest().starts_with('!') && !self.rest().starts_with("!=") {
            self.offset += 1;
            return Ok(IndexPredicate::Not(Box::new(self.unary()?)));
        }
        if self.eat("(") {
            let predicate = self.or()?;
            if !self.eat(")") {
                return Err("expected `)` in index condition".to_string());
            }
            return Ok(predicate);
        }

        let field = self.ident().ok_or_else(|| format!("expected a field in index condition at `{}`", self.rest()))?;
        let operator = [
            ("==", PredicateOperator::Eq),
            ("!=", PredicateOperator::Neq),
            ("<=", PredicateOperator::Lte),
            (">=", PredicateOperator::Gte),
            ("<", PredicateOperator::Lt),
            (">", PredicateOperator::Gt),
        ]
        .into_iter()
        .find_map(|(token, operator)| self.eat(token).then_some(operator));
        let Some(operator) = operator else {
            return Ok(IndexPredicate::Field(field));
        };

        Ok(IndexPredicate::Compare { field, operator, value: self.value()? })
    }

    fn ident(&mut self) -> Option<String> {
        self.skip_whitespace();
        let rest = self.rest();
        let first = rest.chars().next()?;
        if !(first.is_ascii_alphabetic() || first == '_') {
            return None;
        }
        let end =
            rest.find(|character: char| !(character.is_ascii_alphanumeric() || character == '_')).unwrap_or(rest.len());
        let ident = rest[..end].to_string();
        self.offset += end;
        Some(ident)
    }

    fn value(&mut self) -> Result<PredicateValue, String> {
        self.skip_whitespace();
        let rest = self.rest();
        if rest.starts_with('"') {
            let mut value = String::new();
            let mut characters = rest[1..].char_indices();
            while let Some((index, character)) = characters.next() {
                match character {
                    '"' => {
                        self.offset += index + 2;
                        return Ok(PredicateValue::String(value));
                    }
                    '\\' => value.push(match characters.next().map(|(_, character)| character) {
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some(character) => character,
                        None => break,
                    }),
                    character => value.push(character),
                }
            }
            return Err("unterminated string in index condition".to_string());
        }

        let (value, end) = {
            let end = rest
                .find(|character: char| !(character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '-')))
                .unwrap_or(rest.len());
            let token = &rest[..end];
            let value = match token {
                "null" => PredicateValue::Null,
                "true" => PredicateValue::Boolean(true),
                "false" => PredicateValue::Boolean(false),
                number
                    if !number.is_empty()
                        && number.trim_start_matches('-').bytes().all(|byte| byte.is_ascii_digit() || byte == b'.') =>
                {
                    PredicateValue::Number(number.to_string())
                }
                _ => return Err(format!("expected a value in index condition at `{rest}`")),
            };
            (value, end)
        };
        self.offset += end;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compare(field: &str, operator: PredicateOperator, value: PredicateValue) -> IndexPredicate {
        IndexPredicate::Compare { field: field.to_string(), operator, value }
    }

    #[test]
    fn conditions_round_trip_through_their_comment() {
        let predicate = IndexPredicate::And(vec![
            compare("field_1", PredicateOperator::Neq, PredicateValue::Null),
            IndexPredicate::Or(vec![
                compare("field_2", PredicateOperator::Neq, PredicateValue::String("_ \"*/\\ it's".to_string())),
                IndexPredicate::Not(Box::new(IndexPredicate::Field("archived".to_string()))),
                compare("rank", PredicateOperator::Gte, PredicateValue::Number("-1.5".to_string())),
            ]),
            IndexPredicate::Not(Box::new(compare("flag", PredicateOperator::Eq, PredicateValue::Boolean(true)))),
        ]);

        let comment = predicate.comment();
        assert_eq!(
            comment,
            r#"dinoco:where field_1 != null && (field_2 != "_ \"*\/\\ it's" || !archived || rank >= -1.5) && !(flag == true)"#
        );
        assert!(!comment.contains("*/"));
        assert_eq!(IndexPredicate::from_comment(&comment), Some(predicate));
        assert_eq!(IndexPredicate::from_comment("an unrelated comment"), None);
    }

    #[test]
    fn conditions_render_as_sql() {
        let predicate = IndexPredicate::And(vec![
            compare("deleted_at", PredicateOperator::Eq, PredicateValue::Null),
            IndexPredicate::Or(vec![
                compare("status", PredicateOperator::Neq, PredicateValue::String("it's \\".to_string())),
                IndexPredicate::Field("active".to_string()),
            ]),
        ]);
        let quote = |field: &str| format!("\"{field}\"");

        assert_eq!(
            predicate.to_sql(&quote, false),
            r#""deleted_at" IS NULL AND ("status" <> 'it''s \' OR "active" = TRUE)"#
        );
        assert_eq!(
            predicate.to_sql(&quote, true),
            r#""deleted_at" IS NULL AND ("status" <> 'it''s \\' OR "active" = TRUE)"#
        );
        assert_eq!(IndexPredicate::Raw("(a IS NOT NULL)".to_string()).to_string(), "sql((a IS NOT NULL))");
    }
}
