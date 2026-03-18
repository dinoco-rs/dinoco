use pest::Parser;
use pest::error::{Error, ErrorVariant};
use pest::iterators::Pair;

use crate::structs::{Enum, FieldDefaultValue, FieldType, FunctionCall, Schema};
use crate::{EtanolParser, EtanolResult, Field, Rule, Table};

fn format_span_error(message: String, span: pest::Span) -> String {
    let variant = ErrorVariant::CustomError { message };

    Error::<Rule>::new_from_span(variant, span).to_string()
}

fn parse_default_value(value: &str) -> Option<FieldDefaultValue> {
    match value {
        "true" => Some(FieldDefaultValue::Boolean(true)),
        "false" => Some(FieldDefaultValue::Boolean(false)),
        _ => {
            if let Ok(n) = value.parse::<i64>() {
                return Some(FieldDefaultValue::Integer(n));
            }

            if let Ok(f) = value.parse::<f64>() {
                return Some(FieldDefaultValue::Float(f));
            }

            if value.starts_with('"') && value.ends_with('"') {
                return Some(FieldDefaultValue::String(value[1..value.len() - 1].to_string()));
            }

            None
        }
    }
}

fn parse_field(field_pair: Pair<Rule>) -> EtanolResult<Field> {
    let field_span = field_pair.as_span();
    let (field_line, _) = field_span.start_pos().line_col();

    let mut f_inner = field_pair.into_inner();
    let field_name = f_inner.next().unwrap().as_str().to_string();
    let field_type_str = f_inner.next().unwrap().as_str();

    let field_type = FieldType::from_string(field_type_str).map_err(|err| {
        format_span_error(
            format!("Type '{}' does not exist. Did you mean 'String', 'Integer', 'Boolean', or 'Float'?", err),
            field_span,
        )
    })?;

    let mut optional = false;
    let mut unique = false;
    let mut primary_key = false;
    let mut default_value = None;

    for token in f_inner {
        match token.as_rule() {
            Rule::field_optional => optional = true,
            Rule::decorator => {
                let decorator_span = token.as_span();
                let mut attr = token.into_inner();

                let attr_name = attr.next().unwrap().as_str();
                let arg = attr.next();
                let has_args = arg.is_some();

                match (attr_name, has_args) {
                    ("id", true) => return Err(format_span_error("@id does not accept arguments or function calls".to_string(), decorator_span)),
                    ("id", false) => primary_key = true,

                    ("unique", true) => return Err(format_span_error("@unique does not accept arguments or function calls".to_string(), decorator_span)),
                    ("unique", false) => unique = true,

                    ("default", false) => return Err(format_span_error("@default must be used as a function, e.g. @default(value)".to_string(), decorator_span)),
                    ("default", true) => {
                        let value_str = arg.unwrap().as_str();

                        if FunctionCall::is_func(value_str) {
                            let function = FunctionCall::from_string(value_str).map_err(|_| {
                                format_span_error(
                                    "This function does not exist. Did you mean 'uuid()', 'snowflake()', or 'env(\"NAME\")'?".to_string(),
                                    decorator_span,
                                )
                            })?;

                            default_value = Some(FieldDefaultValue::Function(function.into()));
                        } else {
                            default_value = parse_default_value(value_str);
                        }
                    }
                    (unknown, _) => {
                        return Err(format_span_error(
                            format!("Attribute '@{}' does not exist. Did you mean '@id', '@unique' or '@default'?", unknown),
                            decorator_span,
                        ));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(Field {
        name: field_name,
        line: field_line,
        field_type: field_type.into(),

        optional,
        unique,
        primary_key,
        default_value,
    })
}

fn parse_table(table_record: Pair<Rule>) -> EtanolResult<Table> {
    let (table_line, _) = table_record.as_span().start_pos().line_col();
    let mut inner = table_record.into_inner();

    let name = inner.next().unwrap().as_str().to_string();
    let mut fields: Vec<Field> = vec![];

    for field_pair in inner {
        let field_span = field_pair.as_span();
        let field = parse_field(field_pair)?;

        if field.primary_key && fields.iter().any(|x| x.primary_key) {
            return Err(format_span_error(
                "This table has multiple primary keys (@id). A table can only have one.".to_string(),
                field_span,
            ));
        }

        fields.push(field);
    }

    Ok(Table { name, fields, line: table_line })
}

fn parse_enum(enum_record: Pair<Rule>) -> EtanolResult<Enum> {
    let mut inner = enum_record.into_inner();
    let name = inner.next().unwrap().as_str().to_string();

    let mut values: Vec<String> = vec![];

    for value_pair in inner {
        let value_span = value_pair.as_span();
        let value = value_pair.as_str();

        if values.iter().any(|v| v == value) {
            return Err(format_span_error(format!("The value '{}' in enum '{}' is duplicated.", value, name), value_span));
        }

        values.push(value.to_string());
    }

    Ok(Enum { name, values })
}

pub fn parse_schema(raw_input: &str) -> EtanolResult<Schema> {
    let mut parsed = EtanolParser::parse(Rule::schema, raw_input).map_err(|e| format!("Erro de Sintaxe no arquivo schema.etanol:\n{}", e))?;

    let schema_record = parsed.next().unwrap();

    let mut tables: Vec<Table> = vec![];
    let mut enums: Vec<Enum> = vec![];

    for record in schema_record.into_inner() {
        if record.as_rule() == Rule::table_block {}

        match record.as_rule() {
            Rule::table_block => {
                let table = parse_table(record)?;

                tables.push(table);
            }
            Rule::enum_block => {
                let _enum = parse_enum(record)?;

                enums.push(_enum);
            }
            _ => {}
        }
    }

    Ok(Schema { tables, enums })
}
