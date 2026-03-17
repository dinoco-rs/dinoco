use etanol_utils::TableColumn;

use super::{formatType, parseType};

pub fn createFrom(column: &TableColumn) -> String {
    let mut key = format!("\"{}\" => ", column.name);

    let columnType = parseType(column.columnType.clone());

    if column.isOptional {
        key.push_str(&format!(
            r"
        match value.as_str() {{
            {} => _struct.{} = None,
            _ => _struct.{} = Some(value.parse::<{}>().unwrap()),
        }}
    ",
            "\"None\"", column.name, column.name, columnType
        ));

        return key;
    }

    match columnType.as_str() {
        "String" => key.push_str(&format!("_struct.{} = value", column.name)),
        _ => {
            key.push_str(&format!(
                "_struct.{} = Some(value.parse::<{}>().unwrap())",
                column.name, columnType
            ));
        }
    }

    key
}

pub fn createInsertValue(column: &TableColumn) -> String {
    let value = if let Some(value) = &column.default {
        format!("Some(\"{}\".to_string())", value)
    } else if column.uuid {
        "Some(etanol::uuid())".to_string()
    } else {
        "None".to_string()
    };

    format!(
        "(\"{}\".to_string(), Insert::formatType(self.{}.clone(), {}))",
        column.name, column.name, value
    )
}

pub fn createDefault(column: &TableColumn) -> String {
    if column.uuid {
        return format!("{}: Some(etanol::uuid()),", column.name);
    }

    if let Some(value) = &column.default {
        return format!("{}: Some({})", column.name, value);
    }

    if column.isOptional {
        return format!("{}: None", column.name);
    }

    let type_ = match column.columnType.as_str() {
        "String" => "\"\".to_string()".to_string(),
        "Integer" => "0".to_string(),
        "Boolean" => "false".to_string(),
        _ => panic!("Unknown type: {}", column.columnType),
    };

    format!("{}: {}", column.name, type_)
}

pub fn createField(column: &TableColumn) -> String {
    format!("pub {}: {}", column.name, formatType(&column))
}
