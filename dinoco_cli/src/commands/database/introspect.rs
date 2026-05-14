use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use std::{env, fs};

use colored::Colorize;

use dinoco_compiler::compile;
use dinoco_compiler::{
    ConnectionUrl, Database, ParsedConfig, ParsedEnum, ParsedField, ParsedFieldDefault, ParsedFieldType,
    ParsedRelation, ParsedSchema, ParsedTable,
};

use dinoco_codegen::dinoco::render_schema;
use dinoco_formatter::format_from_raw;

use dinoco_engine::{AdapterDialect, DinocoAdapter, DinocoAdapterHandler, DinocoGenericRow, DinocoRow};
use dinoco_engine::{
    DatabaseColumn, DatabaseForeignKey, DatabaseIndex, DinocoClientConfig, DinocoResult, DinocoValue, UniversalAdapter,
};

const SAMPLE_BOOL_LIMIT: usize = 128;

#[derive(Debug, Clone)]
struct IntrospectContext {
    database: Database,
    database_url: ConnectionUrl,
    resolved_url: String,
}

#[derive(Debug, Clone)]
struct IntrospectedField {
    field: ParsedField,
    is_enum_candidate: bool,
    enum_values: Vec<String>,
}

#[derive(Debug, Clone)]
struct ForeignKeyConstraint {
    table_name: String,
    constraint_name: String,
    columns: Vec<String>,
    referenced_table: String,
    referenced_columns: Vec<String>,
}

#[derive(Debug, Clone)]
struct ManyToManyLink {
    join_table: String,
    left_table: String,
    right_table: String,
    relation_name: String,
    left_field_base: Option<String>,
    right_field_base: Option<String>,
}

struct SingleValueRow {
    value: DinocoValue,
}

pub async fn introspect_database() -> DinocoResult<()> {
    println!("{} {}", "✔".green().bold(), "Starting database introspection...".white());

    let context = resolve_introspection_context()?;
    let adapter = UniversalAdapter::connect_for_database(
        &context.database,
        context.resolved_url.clone(),
        DinocoClientConfig::default(),
    )
    .await?;

    println!("{} {}", "✔".green().bold(), "Connected to database.".white());

    let schema = build_schema_from_database(&adapter, &context).await?;
    let raw_schema = render_schema(&schema);
    let formatted_schema = format_from_raw(&raw_schema).unwrap_or(raw_schema);

    fs::create_dir_all("dinoco")?;
    fs::write("dinoco/schema.dinoco", formatted_schema)?;

    println!("{} {}", "✔".green().bold(), "Database introspection completed.".white());
    println!("  {} {}", "→".cyan().bold(), "Generated: dinoco/schema.dinoco".cyan());

    Ok(())
}

fn resolve_introspection_context() -> DinocoResult<IntrospectContext> {
    if Path::new("dinoco/schema.dinoco").exists() {
        let schema_source = fs::read_to_string("dinoco/schema.dinoco")?;

        if let Ok((_, parsed)) = compile(&schema_source) {
            let resolved_url = resolve_connection_url(&parsed.config.database_url)?;

            return Ok(IntrospectContext {
                database: parsed.config.database,
                database_url: parsed.config.database_url,
                resolved_url,
            });
        }
    }

    let database_url = env::var("DATABASE_URL").map_err(|_| {
        dinoco_engine::DinocoError::ParseError(
            "Missing DATABASE_URL and existing schema config is unavailable.".to_string(),
        )
    })?;
    let database = infer_database_from_url(&database_url).ok_or_else(|| {
        dinoco_engine::DinocoError::ParseError(
            "Could not infer database from DATABASE_URL. Use mysql://, postgres://, postgresql:// or file:."
                .to_string(),
        )
    })?;

    Ok(IntrospectContext {
        database,
        database_url: ConnectionUrl::Env("DATABASE_URL".to_string()),
        resolved_url: database_url,
    })
}

fn resolve_connection_url(connection_url: &ConnectionUrl) -> DinocoResult<String> {
    match connection_url {
        ConnectionUrl::Literal(value) => Ok(value.clone()),
        ConnectionUrl::Env(var_name) => env::var(var_name).map_err(|_| {
            dinoco_engine::DinocoError::ParseError(format!("Missing environment variable '{}'.", var_name))
        }),
    }
}

fn infer_database_from_url(url: &str) -> Option<Database> {
    if url.starts_with("postgres://") || url.starts_with("postgresql://") {
        return Some(Database::Postgresql);
    }

    if url.starts_with("mysql://") {
        return Some(Database::Mysql);
    }

    if url.starts_with("file:") {
        return Some(Database::Sqlite);
    }

    None
}

async fn build_schema_from_database(
    adapter: &UniversalAdapter,
    context: &IntrospectContext,
) -> DinocoResult<ParsedSchema> {
    let tables = adapter.fetch_tables().await?;
    let enums = adapter.fetch_enums().await?;
    let indexes = adapter.fetch_indexes().await?;
    let foreign_keys = adapter.fetch_foreign_keys().await?;

    let fk_constraints = group_foreign_keys(&foreign_keys);
    let unique_sets = collect_unique_sets(&tables, &indexes);
    let many_to_many_links = detect_many_to_many_links(&tables, &fk_constraints, &unique_sets);
    let many_to_many_tables = collect_many_to_many_tables(&many_to_many_links);

    let single_column_uniques = collect_single_column_uniques(&indexes);
    let (table_unique_field_sets, table_index_field_sets) = collect_table_level_field_sets(&indexes);
    let db_enums = collect_database_enums(&enums);

    let mut parsed_enums = Vec::new();
    let mut parsed_tables = Vec::new();
    let mut enum_name_by_values: HashMap<String, String> = HashMap::new();

    for (enum_name, enum_values) in &db_enums {
        parsed_enums.push(ParsedEnum { name: enum_name.clone(), values: enum_values.clone() });
        enum_name_by_values.insert(enum_values.join("|"), enum_name.clone());
    }

    for table in tables {
        if many_to_many_tables.contains(&table.name) {
            continue;
        }

        let primary_key_fields = table
            .columns
            .iter()
            .filter(|column| column.is_primary_key)
            .map(|column| column.name.clone())
            .collect::<Vec<_>>();
        let has_single_pk = primary_key_fields.len() == 1;
        let mut fields = Vec::with_capacity(table.columns.len());

        for column in &table.columns {
            let is_unique = single_column_uniques.contains(&(table.name.clone(), column.name.clone()));
            let introspected =
                introspect_column(adapter, &table.name, column, is_unique, has_single_pk, &mut enum_name_by_values)
                    .await?;

            if introspected.is_enum_candidate {
                let enum_name = ensure_enum_for_values(
                    &mut parsed_enums,
                    &mut enum_name_by_values,
                    &table.name,
                    &column.name,
                    &introspected.enum_values,
                );

                let mut field = introspected.field;
                field.field_type = ParsedFieldType::Enum(enum_name);
                fields.push(field);
            } else {
                fields.push(introspected.field);
            }
        }

        parsed_tables.push(ParsedTable {
            name: to_pascal_case(&table.name),
            unique_field_sets: table_unique_field_sets.get(&table.name).cloned().unwrap_or_default(),
            index_field_sets: table_index_field_sets.get(&table.name).cloned().unwrap_or_default(),
            database_name: table.name.clone(),
            primary_key_fields,
            fields,
        });
    }

    apply_foreign_key_relations(&mut parsed_tables, &fk_constraints, &many_to_many_tables, &unique_sets);
    apply_many_to_many_relations(&mut parsed_tables, &many_to_many_links);

    Ok(ParsedSchema {
        config: ParsedConfig {
            database: context.database.clone(),
            database_url: context.database_url.clone(),
            read_replicas: Vec::new(),
            redis: None,
        },
        enums: parsed_enums,
        tables: parsed_tables,
    })
}

fn collect_single_column_uniques(indexes: &[DatabaseIndex]) -> BTreeSet<(String, String)> {
    let mut grouped: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();

    for index in indexes {
        let key = (index.table_name.clone(), index.index_name.clone());

        grouped.entry(key).or_default().push(index.column_name.clone());
    }

    grouped
        .into_iter()
        .filter_map(
            |((table_name, _), columns)| {
                if columns.len() == 1 { Some((table_name, columns[0].clone())) } else { None }
            },
        )
        .collect()
}

fn collect_table_level_field_sets(
    indexes: &[DatabaseIndex],
) -> (HashMap<String, Vec<Vec<String>>>, HashMap<String, Vec<Vec<String>>>) {
    let mut grouped: BTreeMap<(String, String), (bool, Vec<String>)> = BTreeMap::new();
    let mut unique_sets: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    let mut index_sets: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    let mut unique_seen: HashMap<String, HashSet<String>> = HashMap::new();
    let mut index_seen: HashMap<String, HashSet<String>> = HashMap::new();

    for index in indexes {
        let key = (index.table_name.clone(), index.index_name.clone());
        let entry = grouped.entry(key).or_insert((index.is_unique, Vec::new()));

        entry.0 = entry.0 || index.is_unique;
        entry.1.push(index.column_name.clone());
    }

    for ((table_name, _), (is_unique, columns)) in grouped {
        if columns.is_empty() {
            continue;
        }

        let signature = columns_signature(&columns);

        if is_unique {
            if columns.len() > 1 && unique_seen.entry(table_name.clone()).or_default().insert(signature.clone()) {
                unique_sets.entry(table_name.clone()).or_default().push(columns.clone());
            }
        } else if index_seen.entry(table_name.clone()).or_default().insert(signature) {
            index_sets.entry(table_name).or_default().push(columns);
        }
    }

    (unique_sets, index_sets)
}

fn collect_unique_sets(
    tables: &[dinoco_engine::DatabaseParsedTable],
    indexes: &[DatabaseIndex],
) -> HashMap<String, HashSet<String>> {
    let mut result: HashMap<String, HashSet<String>> = HashMap::new();
    let mut grouped_indexes: BTreeMap<(String, String), (bool, Vec<String>)> = BTreeMap::new();

    for table in tables {
        let pk_columns = table
            .columns
            .iter()
            .filter(|column| column.is_primary_key)
            .map(|column| column.name.clone())
            .collect::<Vec<_>>();

        if !pk_columns.is_empty() {
            result.entry(table.name.clone()).or_default().insert(columns_signature(&pk_columns));
        }
    }

    for index in indexes {
        let key = (index.table_name.clone(), index.index_name.clone());
        let entry = grouped_indexes.entry(key).or_insert((index.is_unique, Vec::new()));

        entry.0 = entry.0 || index.is_unique;
        entry.1.push(index.column_name.clone());
    }

    for ((table_name, _), (is_unique, columns)) in grouped_indexes {
        if !is_unique || columns.is_empty() {
            continue;
        }

        result.entry(table_name).or_default().insert(columns_signature(&columns));
    }

    result
}

fn group_foreign_keys(foreign_keys: &[DatabaseForeignKey]) -> Vec<ForeignKeyConstraint> {
    let mut grouped: BTreeMap<(String, String), ForeignKeyConstraint> = BTreeMap::new();

    for foreign_key in foreign_keys {
        let key = (foreign_key.table_name.clone(), foreign_key.constraint_name.clone());
        let entry = grouped.entry(key).or_insert_with(|| ForeignKeyConstraint {
            table_name: foreign_key.table_name.clone(),
            constraint_name: foreign_key.constraint_name.clone(),
            columns: Vec::new(),
            referenced_table: foreign_key.foreign_table_name.clone(),
            referenced_columns: Vec::new(),
        });

        entry.columns.push(foreign_key.column_name.clone());
        entry.referenced_columns.push(foreign_key.foreign_column_name.clone());
    }

    grouped.into_values().collect()
}

fn detect_many_to_many_links(
    tables: &[dinoco_engine::DatabaseParsedTable],
    fk_constraints: &[ForeignKeyConstraint],
    unique_sets: &HashMap<String, HashSet<String>>,
) -> Vec<ManyToManyLink> {
    let mut table_to_fks: HashMap<String, Vec<&ForeignKeyConstraint>> = HashMap::new();
    let table_columns = tables
        .iter()
        .map(|table| {
            (table.name.clone(), table.columns.iter().map(|column| column.name.clone()).collect::<HashSet<_>>())
        })
        .collect::<HashMap<_, _>>();

    for fk in fk_constraints {
        table_to_fks.entry(fk.table_name.clone()).or_default().push(fk);
    }

    let mut links = Vec::new();

    for (table_name, fks) in table_to_fks {
        if fks.len() != 2 {
            continue;
        }

        let Some(columns) = table_columns.get(&table_name) else {
            continue;
        };
        let fk_columns = fks.iter().flat_map(|fk| fk.columns.iter().cloned()).collect::<HashSet<_>>();

        if *columns != fk_columns {
            continue;
        }

        let all_fk_columns = fks.iter().flat_map(|fk| fk.columns.iter().cloned()).collect::<Vec<_>>();

        if !is_unique_column_set(unique_sets, &table_name, &all_fk_columns) {
            continue;
        }

        let relation_name =
            format!("{}{}", to_pascal_case(&fks[0].referenced_table), to_pascal_case(&fks[1].referenced_table),);

        let left_field_base = fks[0].columns.first().map(|column| normalize_relation_field_from_column(column));
        let right_field_base = fks[1].columns.first().map(|column| normalize_relation_field_from_column(column));

        links.push(ManyToManyLink {
            join_table: table_name,
            left_table: fks[0].referenced_table.clone(),
            right_table: fks[1].referenced_table.clone(),
            relation_name,
            left_field_base,
            right_field_base,
        });
    }

    links
}

fn collect_many_to_many_tables(links: &[ManyToManyLink]) -> HashSet<String> {
    let mut result = HashSet::new();

    for link in links {
        result.insert(link.join_table.clone());
    }

    result
}

fn apply_foreign_key_relations(
    parsed_tables: &mut [ParsedTable],
    fk_constraints: &[ForeignKeyConstraint],
    many_to_many_tables: &HashSet<String>,
    unique_sets: &HashMap<String, HashSet<String>>,
) {
    for fk in fk_constraints {
        if many_to_many_tables.contains(&fk.table_name) {
            continue;
        }

        let owner_index = parsed_tables.iter().position(|table| table.database_name == fk.table_name);
        let target_index = parsed_tables.iter().position(|table| table.database_name == fk.referenced_table);
        let (Some(owner_index), Some(target_index)) = (owner_index, target_index) else {
            continue;
        };

        let is_one_to_one = is_unique_column_set(unique_sets, &fk.table_name, &fk.columns);
        let relation_name = relation_name_from_fk(fk, &parsed_tables[owner_index], &parsed_tables[target_index]);

        let owner_optional = fk.columns.iter().any(|column| {
            parsed_tables[owner_index]
                .fields
                .iter()
                .find(|field| field.name == *column)
                .is_some_and(|field| field.is_optional)
        });

        let owner_field_base = fk
            .columns
            .first()
            .map(|column| normalize_relation_field_from_column(column))
            .unwrap_or_else(|| to_field_name(&parsed_tables[target_index].name));
        let owner_field_name = make_unique_field_name(&parsed_tables[owner_index], &owner_field_base);

        let owner_relation = if is_one_to_one {
            ParsedRelation::OneToOneOwner(
                Some(relation_name.clone()),
                fk.columns.clone(),
                fk.referenced_columns.clone(),
                None,
                None,
            )
        } else {
            ParsedRelation::ManyToOne(
                Some(relation_name.clone()),
                fk.columns.clone(),
                fk.referenced_columns.clone(),
                None,
                None,
            )
        };

        let owner_field = ParsedField {
            name: owner_field_name.clone(),
            field_type: ParsedFieldType::Relation(parsed_tables[target_index].name.clone()),
            is_primary_key: false,
            is_optional: owner_optional,
            is_unique: false,
            is_list: false,
            relation: owner_relation,
            default_value: ParsedFieldDefault::NotDefined,
        };

        parsed_tables[owner_index].fields.push(owner_field);

        let reverse_base = if owner_index == target_index {
            if is_one_to_one { format!("{}_inverse", owner_field_name) } else { format!("{}_items", owner_field_name) }
        } else if is_one_to_one {
            to_field_name(&parsed_tables[owner_index].name)
        } else {
            pluralize(&to_field_name(&parsed_tables[owner_index].name))
        };
        let reverse_name = make_unique_field_name(&parsed_tables[target_index], &reverse_base);

        let reverse_relation = if is_one_to_one {
            ParsedRelation::OneToOneInverse(Some(relation_name))
        } else {
            ParsedRelation::OneToMany(Some(relation_name))
        };

        let reverse_field = ParsedField {
            name: reverse_name,
            field_type: ParsedFieldType::Relation(parsed_tables[owner_index].name.clone()),
            is_primary_key: false,
            is_optional: is_one_to_one,
            is_unique: false,
            is_list: !is_one_to_one,
            relation: reverse_relation,
            default_value: ParsedFieldDefault::NotDefined,
        };

        parsed_tables[target_index].fields.push(reverse_field);
    }
}

fn apply_many_to_many_relations(parsed_tables: &mut [ParsedTable], links: &[ManyToManyLink]) {
    for link in links {
        let left_index = parsed_tables.iter().position(|table| table.database_name == link.left_table);
        let right_index = parsed_tables.iter().position(|table| table.database_name == link.right_table);
        let (Some(left_index), Some(right_index)) = (left_index, right_index) else {
            continue;
        };

        let left_base = if left_index == right_index {
            link.left_field_base.clone().unwrap_or_else(|| "left_items".to_string())
        } else {
            pluralize(&to_field_name(&parsed_tables[right_index].name))
        };
        let right_base = if left_index == right_index {
            link.right_field_base.clone().unwrap_or_else(|| "right_items".to_string())
        } else {
            pluralize(&to_field_name(&parsed_tables[left_index].name))
        };

        let left_name = make_unique_field_name(&parsed_tables[left_index], &left_base);
        let right_name = make_unique_field_name(&parsed_tables[right_index], &right_base);

        let left_field = ParsedField {
            name: left_name,
            field_type: ParsedFieldType::Relation(parsed_tables[right_index].name.clone()),
            is_primary_key: false,
            is_optional: false,
            is_unique: false,
            is_list: true,
            relation: ParsedRelation::ManyToMany(Some(link.relation_name.clone())),
            default_value: ParsedFieldDefault::NotDefined,
        };

        parsed_tables[left_index].fields.push(left_field);

        let right_field = ParsedField {
            name: right_name,
            field_type: ParsedFieldType::Relation(parsed_tables[left_index].name.clone()),
            is_primary_key: false,
            is_optional: false,
            is_unique: false,
            is_list: true,
            relation: ParsedRelation::ManyToMany(Some(link.relation_name.clone())),
            default_value: ParsedFieldDefault::NotDefined,
        };

        parsed_tables[right_index].fields.push(right_field);
    }
}

fn relation_name_from_fk(fk: &ForeignKeyConstraint, owner: &ParsedTable, target: &ParsedTable) -> String {
    let constraint = to_pascal_case(&fk.constraint_name);

    if constraint.is_empty() { format!("{}{}", owner.name, target.name) } else { constraint }
}

fn is_unique_column_set(unique_sets: &HashMap<String, HashSet<String>>, table_name: &str, columns: &[String]) -> bool {
    unique_sets.get(table_name).is_some_and(|sets| sets.contains(&columns_signature(columns)))
}

fn columns_signature(columns: &[String]) -> String {
    let mut columns = columns.to_vec();

    columns.sort();

    columns.join("|")
}

fn collect_database_enums(enums: &[dinoco_engine::DatabaseEnumRaw]) -> BTreeMap<String, Vec<String>> {
    let mut grouped = BTreeMap::<String, Vec<String>>::new();

    for value in enums {
        grouped.entry(to_pascal_case(&value.name)).or_default().push(normalize_enum_value(&value.value));
    }

    grouped
}

async fn introspect_column(
    adapter: &UniversalAdapter,
    table_name: &str,
    column: &DatabaseColumn,
    is_unique: bool,
    has_single_pk: bool,
    enum_name_by_values: &mut HashMap<String, String>,
) -> DinocoResult<IntrospectedField> {
    let enum_values = parse_enum_values(column);
    let mut is_enum_candidate = !enum_values.is_empty();
    let normalized_db_type = column.db_type.to_ascii_lowercase();
    let is_primary_key = has_single_pk && column.is_primary_key;
    let mut field_type = map_column_type(&normalized_db_type);

    if is_enum_candidate {
        let key = enum_values.join("|");

        if let Some(enum_name) = enum_name_by_values.get(&key) {
            field_type = ParsedFieldType::Enum(enum_name.clone());
        }
    }

    if matches!(field_type, ParsedFieldType::Integer) {
        if detect_boolean_from_type(&normalized_db_type)
            || detect_boolean_from_data(adapter, table_name, &column.name).await.unwrap_or(false)
        {
            field_type = ParsedFieldType::Boolean;
        }
    }

    if matches!(field_type, ParsedFieldType::Boolean) {
        is_enum_candidate = false;
    }

    Ok(IntrospectedField {
        field: ParsedField {
            name: column.name.clone(),
            field_type,
            is_primary_key,
            is_optional: !is_primary_key && column.nullable,
            is_unique: is_unique && !is_primary_key,
            is_list: false,
            relation: ParsedRelation::NotDefined,
            default_value: ParsedFieldDefault::NotDefined,
        },
        is_enum_candidate,
        enum_values,
    })
}

fn parse_enum_values(column: &DatabaseColumn) -> Vec<String> {
    if let Some(raw_values) = &column.enum_values {
        if raw_values.contains('|') {
            let parsed = raw_values
                .split('|')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(normalize_enum_value)
                .collect::<Vec<_>>();

            if enum_values_are_boolean_like(&parsed) {
                return Vec::new();
            }

            return parsed;
        }
    }

    let lower = column.db_type.to_ascii_lowercase();
    let Some(start) = lower.find("enum(") else {
        return Vec::new();
    };
    let content = &column.db_type[start + 5..];
    let Some(end) = content.rfind(')') else {
        return Vec::new();
    };

    let parsed = content[..end]
        .split(',')
        .map(str::trim)
        .map(|value| value.trim_matches('\'').trim_matches('"'))
        .filter(|value| !value.is_empty())
        .map(normalize_enum_value)
        .collect::<Vec<_>>();

    if enum_values_are_boolean_like(&parsed) {
        return Vec::new();
    }

    parsed
}

fn map_column_type(normalized_db_type: &str) -> ParsedFieldType {
    if detect_boolean_from_type(normalized_db_type) {
        return ParsedFieldType::Boolean;
    }

    if normalized_db_type.contains("json") {
        return ParsedFieldType::Json;
    }

    if normalized_db_type.contains("timestamp") || normalized_db_type.contains("datetime") {
        return ParsedFieldType::DateTime;
    }

    if normalized_db_type == "date" || normalized_db_type.starts_with("date ") {
        return ParsedFieldType::Date;
    }

    if normalized_db_type.contains("int") {
        return ParsedFieldType::Integer;
    }

    if normalized_db_type.contains("double")
        || normalized_db_type.contains("float")
        || normalized_db_type.contains("real")
        || normalized_db_type.contains("decimal")
        || normalized_db_type.contains("numeric")
    {
        return ParsedFieldType::Float;
    }

    ParsedFieldType::String
}

fn detect_boolean_from_type(normalized_db_type: &str) -> bool {
    normalized_db_type.contains("bool") || normalized_db_type == "tinyint(1)" || normalized_db_type == "bit(1)"
}

async fn detect_boolean_from_data(
    adapter: &UniversalAdapter,
    table_name: &str,
    column_name: &str,
) -> DinocoResult<bool> {
    let identifier_column = adapter.dialect().identifier(column_name);
    let identifier_table = adapter.dialect().identifier(table_name);
    let sql = format!(
        "SELECT DISTINCT {identifier_column} FROM {identifier_table} WHERE {identifier_column} IS NOT NULL LIMIT {}",
        SAMPLE_BOOL_LIMIT
    );
    let rows = adapter.query_as::<SingleValueRow>(&sql, &[]).await?;

    if rows.is_empty() {
        return Ok(false);
    }

    Ok(rows.into_iter().all(|row| match row.value {
        DinocoValue::Boolean(_) => true,
        DinocoValue::Integer(value) => value == 0 || value == 1,
        DinocoValue::String(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            normalized == "0" || normalized == "1" || normalized == "true" || normalized == "false"
        }
        _ => false,
    }))
}

fn ensure_enum_for_values(
    parsed_enums: &mut Vec<ParsedEnum>,
    enum_name_by_values: &mut HashMap<String, String>,
    table_name: &str,
    column_name: &str,
    enum_values: &[String],
) -> String {
    let key = enum_values.join("|");

    if let Some(existing_name) = enum_name_by_values.get(&key) {
        return existing_name.clone();
    }

    let mut enum_name = format!("{}{}", to_pascal_case(table_name), to_pascal_case(column_name));

    if parsed_enums.iter().any(|item| item.name == enum_name) {
        let mut index = 2usize;

        loop {
            let candidate = format!("{enum_name}{index}");

            if !parsed_enums.iter().any(|item| item.name == candidate) {
                enum_name = candidate;
                break;
            }

            index += 1;
        }
    }

    parsed_enums.push(ParsedEnum { name: enum_name.clone(), values: enum_values.to_vec() });
    enum_name_by_values.insert(key, enum_name.clone());

    enum_name
}

fn normalize_enum_value(value: &str) -> String {
    let normalized = value
        .trim()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch.to_ascii_uppercase() } else { '_' })
        .collect::<String>();
    let collapsed = normalized.split('_').filter(|piece| !piece.is_empty()).collect::<Vec<_>>().join("_");

    if collapsed.is_empty() { "UNKNOWN".to_string() } else { collapsed }
}

fn enum_values_are_boolean_like(values: &[String]) -> bool {
    !values.is_empty() && values.iter().all(|value| value == "0" || value == "1" || value == "TRUE" || value == "FALSE")
}

fn to_pascal_case(value: &str) -> String {
    let mut result = String::new();

    for piece in
        value.chars().map(|ch| if ch.is_alphanumeric() { ch } else { ' ' }).collect::<String>().split_whitespace()
    {
        let mut chars = piece.chars();

        if let Some(first) = chars.next() {
            result.push(first.to_ascii_uppercase());

            for ch in chars {
                result.push(ch.to_ascii_lowercase());
            }
        }
    }

    if result.is_empty() { "Model".to_string() } else { result }
}

fn to_field_name(value: &str) -> String {
    let pascal = to_pascal_case(value);
    let mut chars = pascal.chars();

    if let Some(first) = chars.next() {
        let mut result = String::new();
        result.push(first.to_ascii_lowercase());
        result.push_str(chars.as_str());

        if result.is_empty() { "field".to_string() } else { result }
    } else {
        "field".to_string()
    }
}

fn pluralize(value: &str) -> String {
    if value.ends_with('s') { format!("{value}List") } else { format!("{value}s") }
}

fn normalize_relation_field_from_column(column: &str) -> String {
    let trimmed = column.strip_suffix("_id").unwrap_or(column).strip_suffix("Id").unwrap_or(column);

    to_field_name(trimmed)
}

fn make_unique_field_name(table: &ParsedTable, base: &str) -> String {
    if !table.fields.iter().any(|field| field.name == base) {
        return base.to_string();
    }

    let mut index = 2usize;

    loop {
        let candidate = format!("{base}{index}");

        if !table.fields.iter().any(|field| field.name == candidate) {
            return candidate;
        }

        index += 1;
    }
}

impl DinocoRow for SingleValueRow {
    fn from_row<R: DinocoGenericRow>(row: &R) -> DinocoResult<Self> {
        Ok(Self { value: row.get_value(0)? })
    }
}
