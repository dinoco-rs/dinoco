use pest::Parser;
use pest::iterators::Pair;
use pest_derive::Parser;

use crate::ast::{
    Attribute, AttributeArgument, AttributeValue, ConfigBlock, ConfigEntry, ConfigValue, EnumDef, FieldType, Model,
    ModelField, Schema, SchemaItem,
};
use crate::error::CompileError;

pub type CompileResult<T> = Result<T, CompileError>;

#[derive(Parser)]
#[grammar = "schema.pest"]
struct DinocoPestParser;

pub fn parse_schema(source: &str) -> CompileResult<Schema> {
    let mut pairs = DinocoPestParser::parse(Rule::schema, source).map_err(CompileError::from)?;
    let schema = pairs.next().expect("schema pair");
    let mut items = Vec::new();

    for pair in schema.into_inner() {
        match pair.as_rule() {
            Rule::config_block => items.push(SchemaItem::Config(parse_config(pair)?)),
            Rule::enum_block => items.push(SchemaItem::Enum(parse_enum(pair)?)),
            Rule::model_block => items.push(SchemaItem::Model(parse_model(pair)?)),
            Rule::EOI => {}
            _ => return Err(pair_error(&pair, "unexpected schema item")),
        }
    }

    let schema = Schema { items };
    validate_schema(&schema)?;
    Ok(schema)
}

fn parse_config(pair: Pair<'_, Rule>) -> CompileResult<ConfigBlock> {
    let mut entries = Vec::new();

    for pair in pair.into_inner() {
        if pair.as_rule() == Rule::config_entry {
            entries.push(parse_config_entry(pair)?);
        }
    }

    Ok(ConfigBlock { entries })
}

fn parse_config_entry(pair: Pair<'_, Rule>) -> CompileResult<ConfigEntry> {
    let position_pair = pair.clone();
    let mut inner = pair.into_inner();
    let key = expect_rule(&mut inner, Rule::ident, "expected config key")?.as_str().to_string();
    let value = parse_config_value(expect_rule(&mut inner, Rule::config_value, "expected config value")?)?;

    validate_config_entry_value(&key, &value, &position_pair)?;

    Ok(ConfigEntry { key, value })
}

fn validate_config_entry_value(key: &str, value: &ConfigValue, pair: &Pair<'_, Rule>) -> CompileResult<()> {
    match (key, value) {
        ("database_url", ConfigValue::Env(_)) => Ok(()),
        ("database_url", _) => Err(pair_error(pair, "`database_url` only accepts env(\"DATABASE_URL\")")),
        ("read_replicas", ConfigValue::Array(values)) => {
            if values.iter().all(|value| matches!(value, ConfigValue::Env(_))) {
                Ok(())
            } else {
                Err(pair_error(pair, "`read_replicas` only accepts env(...) values"))
            }
        }
        ("read_replicas", _) => Err(pair_error(pair, "`read_replicas` must be an array of env(...) values")),
        ("snowflake_node_id", ConfigValue::Env(_)) => Ok(()),
        ("snowflake_node_id", _) => Err(pair_error(pair, "`snowflake_node_id` only accepts env(...)")),
        _ => Ok(()),
    }
}

fn validate_schema(schema: &Schema) -> CompileResult<()> {
    let mut uses_snowflake = false;

    for model in schema.models() {
        for field in &model.fields {
            if field.is_relation(schema) {
                if let Some(relation) = field.attributes.iter().find(|attribute| attribute.name == "relation") {
                    for action_name in ["onDelete", "onUpdate"] {
                        let Some(action) = relation.argument(action_name) else {
                            continue;
                        };
                        let Some(action) = attribute_string_or_ident(action) else {
                            return Err(CompileError::new(
                                format!(
                                    "{action_name} must be one of: Cascade, Restrict, NoAction, SetNull, SetDefault"
                                ),
                                1,
                                1,
                            ));
                        };
                        if !matches!(action, "Cascade" | "Restrict" | "NoAction" | "SetNull" | "SetDefault") {
                            return Err(CompileError::new(
                                format!(
                                    "{action_name} must be one of: Cascade, Restrict, NoAction, SetNull, SetDefault"
                                ),
                                1,
                                1,
                            ));
                        }
                        if action == "SetNull" && !field.ty.optional {
                            return Err(CompileError::new(
                                format!("{action_name}: SetNull requires an optional relation field"),
                                1,
                                1,
                            ));
                        }
                    }
                }
            }

            let Some(default) = field.attributes.iter().find(|attribute| attribute.name == "default") else {
                continue;
            };
            let Some(AttributeArgument::Value(value)) = default.arguments.first() else {
                continue;
            };

            if let AttributeValue::Call { name, .. } = value {
                match name.as_str() {
                    "uuid" if field.ty.name != "String" => {
                        return Err(CompileError::new("uuid() defaults are only supported for String fields", 1, 1));
                    }
                    "snowflake" if field.ty.name != "Integer" => {
                        return Err(CompileError::new(
                            "snowflake() defaults are only supported for Integer fields",
                            1,
                            1,
                        ));
                    }
                    "autoincrement" if field.ty.name != "Integer" => {
                        return Err(CompileError::new(
                            "autoincrement() defaults are only supported for Integer fields",
                            1,
                            1,
                        ));
                    }
                    "now" if field.ty.name != "DateTime" && field.ty.name != "Date" => {
                        return Err(CompileError::new(
                            "now() defaults are only supported for DateTime or Date fields",
                            1,
                            1,
                        ));
                    }
                    "snowflake" => uses_snowflake = true,
                    "uuid" | "autoincrement" | "now" => {}
                    _ => {
                        return Err(CompileError::new(
                            "unsupported @default() function. Supported: autoincrement(), uuid(), snowflake(), now()",
                            1,
                            1,
                        ));
                    }
                }
            }
        }
    }

    if uses_snowflake {
        let has_node_id = schema
            .config()
            .into_iter()
            .flat_map(|config| &config.entries)
            .any(|entry| entry.key == "snowflake_node_id" && matches!(entry.value, ConfigValue::Env(_)));
        if !has_node_id {
            return Err(CompileError::new("snowflake() requires config.snowflake_node_id = env(\"...\")", 1, 1));
        }
    }

    Ok(())
}

fn attribute_string_or_ident(value: &AttributeValue) -> Option<&str> {
    match value {
        AttributeValue::String(value) | AttributeValue::Ident(value) => Some(value),
        _ => None,
    }
}

fn parse_config_value(pair: Pair<'_, Rule>) -> CompileResult<ConfigValue> {
    let error = pair_position_error(&pair, "expected config value");
    let value = pair.into_inner().next().ok_or(error)?;

    match value.as_rule() {
        Rule::config_array => {
            let values = value.into_inner().map(parse_config_value).collect::<CompileResult<Vec<_>>>()?;
            Ok(ConfigValue::Array(values))
        }
        Rule::env_call => {
            let error = pair_position_error(&value, "expected env name");
            let raw = value.into_inner().find(|pair| pair.as_rule() == Rule::string_literal).ok_or(error)?;
            Ok(ConfigValue::Env(unquote(raw.as_str())))
        }
        Rule::string_literal => Ok(ConfigValue::String(unquote(value.as_str()))),
        Rule::ident => Ok(ConfigValue::Ident(value.as_str().to_string())),
        _ => Err(pair_error(&value, "unexpected config value")),
    }
}

fn parse_enum(pair: Pair<'_, Rule>) -> CompileResult<EnumDef> {
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "expected enum name")?.as_str().to_string();
    let values = inner
        .filter(|pair| pair.as_rule() == Rule::enum_value)
        .map(|pair| {
            pair.into_inner()
                .next()
                .map(|pair| pair.as_str().to_string())
                .ok_or_else(|| CompileError::new("expected enum value", 1, 1))
        })
        .collect::<CompileResult<Vec<_>>>()?;

    Ok(EnumDef { name, values })
}

fn parse_model(pair: Pair<'_, Rule>) -> CompileResult<Model> {
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "expected model name")?.as_str().to_string();
    let fields = inner
        .filter(|pair| pair.as_rule() == Rule::model_field)
        .map(parse_model_field)
        .collect::<CompileResult<Vec<_>>>()?;

    Ok(Model { name, fields })
}

fn parse_model_field(pair: Pair<'_, Rule>) -> CompileResult<ModelField> {
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "expected field name")?.as_str().to_string();
    let ty_name = expect_rule(&mut inner, Rule::field_type, "expected field type")?
        .into_inner()
        .next()
        .ok_or_else(|| CompileError::new("expected field type", 1, 1))?
        .as_str()
        .to_string();
    let mut optional = false;
    let mut list = false;
    let mut attributes = Vec::new();

    for pair in inner {
        match pair.as_rule() {
            Rule::field_optional => optional = true,
            Rule::field_list => list = true,
            Rule::attribute => attributes.push(parse_attribute(pair)?),
            _ => return Err(pair_error(&pair, "unexpected field token")),
        }
    }

    if list {
        optional = false;
    }

    Ok(ModelField { name, ty: FieldType { name: ty_name, optional, list }, attributes })
}

fn parse_attribute(pair: Pair<'_, Rule>) -> CompileResult<Attribute> {
    let mut inner = pair.into_inner();
    let name = expect_rule(&mut inner, Rule::ident, "expected attribute name")?.as_str().to_string();
    let arguments = inner
        .find(|pair| pair.as_rule() == Rule::attribute_arguments)
        .map(parse_attribute_arguments)
        .transpose()?
        .unwrap_or_default();

    Ok(Attribute { name, arguments })
}

fn parse_attribute_arguments(pair: Pair<'_, Rule>) -> CompileResult<Vec<AttributeArgument>> {
    pair.into_inner().filter(|pair| pair.as_rule() == Rule::attribute_argument).map(parse_attribute_argument).collect()
}

fn parse_attribute_argument(pair: Pair<'_, Rule>) -> CompileResult<AttributeArgument> {
    let error = pair_position_error(&pair, "expected attribute argument");
    let pair = pair.into_inner().next().ok_or(error)?;

    match pair.as_rule() {
        Rule::named_argument => {
            let mut inner = pair.into_inner();
            let key = expect_rule(&mut inner, Rule::ident, "expected argument name")?.as_str().to_string();
            let value =
                parse_attribute_value(expect_rule(&mut inner, Rule::attribute_value, "expected argument value")?)?;

            Ok(AttributeArgument::Named { key, value })
        }
        Rule::attribute_value => Ok(AttributeArgument::Value(parse_attribute_value(pair)?)),
        _ => Err(pair_error(&pair, "unexpected attribute argument")),
    }
}

fn parse_attribute_value(pair: Pair<'_, Rule>) -> CompileResult<AttributeValue> {
    let error = pair_position_error(&pair, "expected attribute value");
    let pair = pair.into_inner().next().ok_or(error)?;

    match pair.as_rule() {
        Rule::attribute_array => {
            let values = pair
                .into_inner()
                .filter(|pair| pair.as_rule() == Rule::attribute_value)
                .map(parse_attribute_value)
                .collect::<CompileResult<Vec<_>>>()?;

            Ok(AttributeValue::Array(values))
        }
        Rule::attribute_call => {
            let mut inner = pair.into_inner();
            let name = expect_rule(&mut inner, Rule::ident, "expected function name")?.as_str().to_string();
            let arguments = inner
                .find(|pair| pair.as_rule() == Rule::attribute_arguments)
                .map(parse_attribute_arguments)
                .transpose()?
                .unwrap_or_default();

            Ok(AttributeValue::Call { name, arguments })
        }
        Rule::string_literal => Ok(AttributeValue::String(unquote(pair.as_str()))),
        Rule::number_literal | Rule::boolean_literal | Rule::ident => {
            Ok(AttributeValue::Ident(pair.as_str().to_string()))
        }
        _ => Err(pair_error(&pair, "unexpected attribute value")),
    }
}

fn expect_rule<'a>(
    inner: &mut impl Iterator<Item = Pair<'a, Rule>>,
    rule: Rule,
    message: &'static str,
) -> CompileResult<Pair<'a, Rule>> {
    let pair = inner.next().ok_or_else(|| CompileError::new(message, 1, 1))?;

    if pair.as_rule() == rule { Ok(pair) } else { Err(pair_error(&pair, message)) }
}

fn pair_error(pair: &Pair<'_, Rule>, message: impl Into<String>) -> CompileError {
    let (line, column) = pair.as_span().start_pos().line_col();
    CompileError::new(message, line, column)
}

fn pair_position_error(pair: &Pair<'_, Rule>, message: impl Into<String>) -> CompileError {
    pair_error(pair, message)
}

fn unquote(value: &str) -> String {
    let value = value.strip_prefix('"').and_then(|value| value.strip_suffix('"')).unwrap_or(value);
    let mut output = String::new();
    let mut chars = value.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        output.push(match chars.next() {
            Some('"') => '"',
            Some('\\') => '\\',
            Some('n') => '\n',
            Some('r') => '\r',
            Some('t') => '\t',
            Some(other) => other,
            None => '\\',
        });
    }

    output
}

#[cfg(test)]
mod tests {
    use super::parse_schema;

    #[test]
    fn compiles_schema_model() {
        let schema = parse_schema(
            r#"
            config {
                database = "postgresql"
                database_url = env("DATABASE_URL")
                read_replicas = [env("DATABASE_URL"), env("READ_DATABASE_URL")]
            }

            enum Status {
                Active
                Canceled
            }

            model User {
                id      String  @id @default(uuid())
                email   String
                status  Status
                tokens  Token[]
            }

            model Token {
                id       String  @id @default(uuid())
                user     User?   @relation(fields: [user_id], references: [id], onDelete: Cascade)
                user_id  String?
            }
            "#,
        )
        .expect("schema should compile");

        assert_eq!(schema.config().expect("config").entries.len(), 3);
        assert_eq!(schema.enums().count(), 1);
        assert_eq!(schema.models().count(), 2);

        let token = schema.models().find(|model| model.name == "Token").expect("token");
        let user = token.fields.iter().find(|field| field.name == "user").expect("user");
        assert!(user.ty.optional);
        assert!(user.attributes.iter().any(|attribute| attribute.name == "relation"));
    }
}
