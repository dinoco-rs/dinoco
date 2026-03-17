use etanol_utils::TableColumn;

pub fn formatType(column: &TableColumn) -> String {
    let type_ = parseType(column.columnType.clone());

    if column.isOptional | column.uuid | column.autoincrement | column.default.is_some() {
        return format!("Option<{}>", type_);
    }

    type_
}

pub fn parseType(type_: String) -> String {
    match type_.as_str() {
        "String" => "String".to_string(),
        "Integer" => "i64".to_string(),
        "Boolean" => "bool".to_string(),
        _ => panic!("Unknown type: {}", type_),
    }
}
