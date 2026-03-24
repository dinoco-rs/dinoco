use std::collections::HashMap;

use pest::Parser;
use pest::iterators::Pair;

use crate::ast::*;
use crate::{DinocoParser, Rule};

fn is_keyword(value: &str) -> bool {
    let keywords = vec!["config", "model", "enum"];

    keywords.contains(&value)
}

fn parse_default_value(value: &str) -> FieldDefaultValue {
    match value {
        "true" => FieldDefaultValue::Boolean(true),
        "false" => FieldDefaultValue::Boolean(false),
        _ => {
            if let Ok(n) = value.parse::<i64>() {
                return FieldDefaultValue::Integer(n);
            }
            if let Ok(f) = value.parse::<f64>() {
                return FieldDefaultValue::Float(f);
            }
            if value.starts_with('"') && value.ends_with('"') {
                return FieldDefaultValue::String(value[1..value.len() - 1].to_string());
            }

            FieldDefaultValue::Custom(value.to_string())
        }
    }
}

fn parse_field_type(data: &str) -> FieldType {
    match data {
        "Boolean" => FieldType::Boolean,
        "String" => FieldType::String,
        "Integer" => FieldType::Integer,
        "Float" => FieldType::Float,
        custom => FieldType::Custom(custom.to_string()),
    }
}

fn parse_field<'a>(field_pair: Pair<'a, Rule>, position: usize) -> DinocoResult<Field<'a>> {
    let span = field_pair.as_span();
    let mut f_inner = field_pair.into_inner();

    let name = f_inner.next().unwrap().as_str().to_string();
    let field_type_str = f_inner.next().unwrap().as_str();
    let field_type = parse_field_type(field_type_str);

    let mut is_optional = false;
    let mut is_unique = false;
    let mut is_primary_key = false;
    let mut is_list = false;
    let mut default_value = FieldDefaultValue::NotDefined;
    let mut relation = None;
    let mut newlines = 0;
    let mut comments = vec![];

    for token in f_inner {
        match token.as_rule() {
            Rule::COMMENT => comments.push(token.as_str().to_string()),
            Rule::NEWLINE => {
                newlines += 1;
            }
            Rule::field_optional => is_optional = true,
            Rule::array_open => is_list = true,

            Rule::decorator => {
                let decorator_span = token.as_span();

                let mut attr_name = String::new();
                let mut param_value: Option<String> = None;
                let mut has_args = false;

                let mut named_params: HashMap<String, Vec<String>> = HashMap::new();

                for attr_token in token.into_inner() {
                    match attr_token.as_rule() {
                        Rule::ident => attr_name = attr_token.as_str().to_string(),
                        Rule::paren_open => has_args = true,
                        Rule::param => param_value = Some(attr_token.as_str().to_string()),

                        Rule::named_param => {
                            let mut param_name = String::new();
                            let mut values = Vec::new();

                            for token in attr_token.clone().into_inner() {
                                match token.as_rule() {
                                    Rule::ident => {
                                        param_name = token.as_str().to_string();
                                    }

                                    Rule::named_value => {
                                        for inner in token.into_inner() {
                                            match inner.as_rule() {
                                                Rule::ident | Rule::string_literal => {
                                                    values.push(inner.as_str().to_string());
                                                }

                                                Rule::named_array => {
                                                    for item in inner.into_inner() {
                                                        if let Rule::ident = item.as_rule() {
                                                            values.push(item.as_str().to_string());
                                                        }
                                                    }
                                                }

                                                _ => {}
                                            }
                                        }
                                    }

                                    _ => {}
                                }
                            }

                            if named_params.contains_key(&param_name) {
                                return Err(format_span_error(
                                    format!("Duplicate argument '{}' in @relation. Each argument can only be defined once.", param_name),
                                    decorator_span,
                                ));
                            }

                            named_params.insert(param_name, values);
                        }
                        _ => {}
                    }
                }

                match (attr_name.as_str(), has_args) {
                    ("id", true) => return Err(format_span_error("@id does not accept arguments".to_string(), decorator_span)),
                    ("id", false) => is_primary_key = true,

                    ("unique", true) => return Err(format_span_error("@unique does not accept arguments".to_string(), decorator_span)),
                    ("unique", false) => is_unique = true,

                    ("relation", true) => {
                        relation = Some(Relation {
                            named_params,
                            span: decorator_span,
                        });
                    }
                    ("relation", false) => {
                        return Err(format_span_error(
                            "@relation requires arguments e.g: (fields: [userId], references: [id])".to_string(),
                            decorator_span,
                        ));
                    }

                    ("default", false) => return Err(format_span_error("@default must be used as a function or value".to_string(), decorator_span)),
                    ("default", true) => {
                        if let Some(value_str) = param_value {
                            if FunctionCall::is_func(&value_str) {
                                let function = FunctionCall::from_string(&value_str).map_err(|_| {
                                    format_span_error(
                                        "This function does not exist. Try uuid(), autoincrement(), snowflake(), or env(\"...\").".to_string(),
                                        decorator_span,
                                    )
                                })?;

                                default_value = FieldDefaultValue::Function(function);
                            } else {
                                default_value = parse_default_value(&value_str);
                            }
                        } else {
                            return Err(format_span_error(
                                "@default() requires a value inside the parentheses. Named parameters are not supported.".to_string(),
                                decorator_span,
                            ));
                        }
                    }
                    (unknown, _) => return Err(format_span_error(format!("Attribute '@{}' does not exist.", unknown), decorator_span)),
                }
            }
            _ => {}
        }
    }

    Ok(Field {
        name,
        field_type,
        is_optional,
        is_unique,
        is_list,
        is_primary_key,
        default_value,
        relation,
        span,

        newlines,

        position,
        comments,
    })
}

fn parse_table<'a>(table_record: Pair<'a, Rule>, position: usize) -> DinocoResult<Table<'a>> {
    let span = table_record.as_span();

    let mut name = String::new();
    let mut fields = vec![];

    let mut comments = vec![];

    let inner = table_record.into_inner();
    let total_fields = inner.len();

    for (i, pair) in inner.enumerate() {
        match pair.as_rule() {
            Rule::COMMENT => {
                comments.push((i, pair.as_span()));
            }

            Rule::ident => {
                name = pair.as_str().to_string();

                if is_keyword(&name) {
                    return Err(format_span_error(
                        format!("Invalid model name '{}': this identifier is a reserved keyword.", name),
                        pair.as_span(),
                    ));
                }
            }
            Rule::field => fields.push(parse_field(pair, i)?),
            _ => {}
        }
    }

    Ok(Table {
        position,
        total_fields,

        name,
        fields,
        span,
        comments,
    })
}

fn parse_enum<'a>(enum_record: Pair<'a, Rule>, position: usize) -> DinocoResult<Enum<'a>> {
    let span = enum_record.as_span();

    let mut name = String::new();
    let mut values = vec![];
    let mut comments = vec![];

    let inner = enum_record.into_inner();
    let total_blocks = inner.len();

    for (i, pair) in inner.enumerate() {
        match pair.as_rule() {
            Rule::COMMENT => {
                comments.push((i, pair.as_span()));
            }
            Rule::ident => {
                if name.is_empty() {
                    name = pair.as_str().to_string();

                    if is_keyword(&name) {
                        return Err(format_span_error(
                            format!("Invalid enum name '{}': this identifier is a reserved keyword.", name),
                            pair.as_span(),
                        ));
                    }
                } else {
                    values.push((i, pair.as_span()));
                }
            }
            _ => {}
        }
    }

    if values.is_empty() {
        return Err(format_span_error(format!("Enum '{}' must have at least one value.", name), span));
    }

    Ok(Enum {
        total_blocks,
        position,
        comments,
        name,
        values,
        span,
    })
}

fn parse_config<'a>(config_record: Pair<'a, Rule>, position: usize) -> DinocoResult<Config<'a>> {
    let span = config_record.as_span();
    let mut fields = vec![];

    let inner = config_record.into_inner();
    let total_fields = inner.len();
    let mut comments = vec![];

    for (i, pair) in inner.enumerate() {
        match pair.as_rule() {
            Rule::config_field => fields.push(parse_config_field(pair, i)?),
            Rule::COMMENT => comments.push((i, pair.as_span())),
            _ => {}
        }
    }

    Ok(Config {
        total_fields,
        position,
        comments,
        fields,
        span,
    })
}

fn parse_config_field<'a>(field_record: Pair<'a, Rule>, position: usize) -> DinocoResult<ConfigField<'a>> {
    let span = field_record.as_span();
    let mut name = String::new();
    let mut value = None;

    let mut comments = vec![];

    for pair in field_record.into_inner() {
        match pair.as_rule() {
            Rule::COMMENT => comments.push(pair.as_span()),
            Rule::ident => name = pair.as_str().to_string(),
            Rule::config_param => {
                let inner = pair.into_inner().next().unwrap();

                value = Some(parse_config_value(inner)?);
            }
            _ => {}
        }
    }

    Ok(ConfigField {
        position,
        comments,
        name,
        value,
        span,
    })
}

fn parse_config_value<'a>(value_record: Pair<'a, Rule>) -> DinocoResult<ConfigValue<'a>> {
    match value_record.as_rule() {
        Rule::string_literal => {
            let content = value_record.into_inner().next().unwrap().as_str();

            Ok(ConfigValue::String(content.to_string()))
        }

        Rule::config_array => {
            let mut items = vec![];

            for item in value_record.into_inner() {
                if item.as_rule() == Rule::config_array_value {
                    let inner = item.into_inner().next().unwrap();

                    items.push(parse_config_value(inner)?);
                }
            }

            Ok(ConfigValue::Array(items))
        }
        Rule::config_object => {
            let mut fields = vec![];

            for pair in value_record.into_inner() {
                if pair.as_rule() == Rule::config_field {
                    fields.push(parse_config_field(pair, 1)?);
                }
            }

            Ok(ConfigValue::Object(fields))
        }
        Rule::function => {
            let mut inner = value_record.into_inner();

            let name = inner.next().unwrap().as_str().to_string();
            let mut args = vec![];

            for param_pair in inner {
                match param_pair.as_rule() {
                    Rule::paren_open => {}
                    Rule::paren_close => {}
                    _ => {
                        args.push(parse_config_value(param_pair.into_inner().next().unwrap())?);
                    }
                }
            }

            Ok(ConfigValue::Function { name, args })
        }
        _ => Err(format_span_error("Invalid config value".to_string(), value_record.as_span())),
    }
}

pub fn parse_schema<'a>(raw_input: &'a str) -> DinocoResult<Schema<'a>> {
    let mut parsed = DinocoParser::parse(Rule::schema, raw_input).map_err(|e| {
        let (start_line, start_column, end_line, end_column) = match e.line_col {
            pest::error::LineColLocation::Pos((line, col)) => (line, col, line, col + 1),
            pest::error::LineColLocation::Span((start_line, start_col), (end_line, end_col)) => (start_line, start_col, end_line, end_col),
        };

        let err = e
            .renamed_rules(|rule| {
                match rule {
                    Rule::WHITESPACE => "whitespace",
                    Rule::INLINE_WHITESPACE => "inline whitespace",
                    Rule::NEWLINE => "newline",
                    Rule::COMMENT => "a comment (starting with #)",

                    Rule::model_keyword => "the 'model' keyword",
                    Rule::enum_keyword => "the 'enum' keyword",
                    Rule::config_keyword => "the 'config' keyword",

                    Rule::block_open => "an opening brace '{'",
                    Rule::block_close => "a closing brace '}'",
                    Rule::paren_open => "an opening parenthesis '('",
                    Rule::paren_close => "a closing parenthesis ')'",
                    Rule::array_open => "an opening bracket '['",
                    Rule::array_close => "a closing bracket ']'",
                    Rule::decorator_prefix => "the decorator symbol '@'",
                    Rule::array_separator => "a comma ','",
                    Rule::named_separator => "a colon ':'",
                    Rule::config_separator => "an equals sign '='",
                    Rule::field_optional => "the optional marker '?'",

                    Rule::ident => "a valid identifier (e.g., User, email, or My_Table)",
                    Rule::inner_string => "text content inside quotes",
                    Rule::number_literal => "a valid number",
                    Rule::string_literal => "a quoted string (e.g., \"...\")",
                    Rule::boolean_literal => "a boolean (true or false)",

                    Rule::function => "a function call (e.g., env(\"...\"))",
                    Rule::decorator => "a decorator (e.g., @id or @default(...))",
                    Rule::param => "a valid parameter (string, number, boolean, or function)",
                    Rule::field_type => "a field type (e.g., String, Int, or a Model name)",
                    Rule::field => "a field declaration (e.g., name String @id)",

                    Rule::named_array => "an array of identifiers (e.g., [A, B, C])",
                    Rule::named_value => "a named value (identifier, string, or array)",
                    Rule::named_param => "a named parameter (e.g., key: value)",

                    Rule::config_object => "a configuration object '{ ... }'",
                    Rule::config_array_value => "a string, function, or config object inside an array",
                    Rule::config_array => "a configuration array (e.g., [ ... ])",
                    Rule::config_param => "a configuration value (string, function, array, or config field)",
                    Rule::config_field => "a configuration field assignment (e.g., key = value)",

                    Rule::model_block => "a model block definition",
                    Rule::enum_block => "an enum block definition",
                    Rule::config_block => "a config block definition",

                    Rule::schema => "a valid dinoco schema definition",
                    Rule::EOI => "the end of the file",

                    _ => "a valid token",
                }
                .to_string()
            })
            .variant
            .message()
            .to_string();

        vec![DinocoError {
            message: err,
            start_line,
            start_column,
            end_line,
            end_column,
        }]
    })?;

    let schema_record = parsed.next().unwrap();
    let span = schema_record.as_span();

    let mut comments = vec![];
    let mut configs = vec![];
    let mut tables = vec![];
    let mut enums = vec![];

    let inner = schema_record.into_inner();

    let total_blocks = inner.len();

    for (i, record) in inner.enumerate() {
        match record.as_rule() {
            Rule::model_block => tables.push(parse_table(record, i)?),
            Rule::enum_block => enums.push(parse_enum(record, i)?),
            Rule::config_block => configs.push(parse_config(record, i)?),
            Rule::COMMENT => comments.push((i, record.as_str().to_string())),
            _ => {}
        }
    }

    Ok(Schema {
        tables,
        enums,
        configs,
        span,
        comments,
        total_blocks,
    })
}

pub fn format_span_error(message: String, span: pest::Span) -> Vec<DinocoError> {
    let (start_line, start_column) = span.start_pos().line_col();
    let (end_line, end_column) = span.end_pos().line_col();

    vec![DinocoError {
        message: format!("{}", message),

        start_line,
        start_column,

        end_line,
        end_column,
    }]
}

pub fn format_span_errors(data: Vec<(String, pest::Span)>) -> Vec<DinocoError> {
    let mut errors = vec![];

    for (message, span) in data {
        let (start_line, start_column) = span.start_pos().line_col();
        let (end_line, end_column) = span.end_pos().line_col();

        errors.push(DinocoError {
            message: format!("{}", message),

            start_line,
            start_column,

            end_line,
            end_column,
        });
    }

    errors
}
