use crate::utils::{ParsedToken, Pointer, TableColumn, Token};

mod methods;
use methods::readDecorator;

fn readDecorators(
    pointer: &mut Pointer,
    columnType: String,
    columns: &Vec<TableColumn>,
) -> Vec<(String, String)> {
    let mut decorators: Vec<(String, String)> = vec![];

    loop {
        if let Some(result) = readDecorator(pointer, columnType.clone(), columns, &decorators) {
            decorators.push(result);

            continue;
        }

        break;
    }

    return decorators;
}

pub fn table(pointer: &mut Pointer) {
    pointer.take("Keyword");

    let tableName = pointer.take("Identifier");
    if tableName.is_none() {
        pointer.error("Expected a 'Identifier'");
    }

    let startBracket = pointer.toEqual("Brackets", "{");
    if !startBracket {
        pointer.error(&format!("Expected a '{}'", "{"));
    }

    let mut columns: Vec<TableColumn> = Vec::new();

    loop {
        if pointer.toEqual("Brackets", "}") {
            break;
        }

        match pointer.token.clone() {
            None | Some(Token::EOF) => {
                pointer.error(&format!("Expected a '{}'", "}"));
            }
            Some(Token::Identifier(name)) => {
                if columns.iter().find(|x| x.name == name).is_some() {
                    pointer.error(&format!("duplicate column '{}'", name));
                }

                match pointer.previewNext(true) {
                    Some(Token::TableType(_)) => {}
                    _ => {
                        pointer.error(&format!(
                            "Expected a '{}'",
                            pointer.tokenizer.tableTypes.join(" or ")
                        ));
                    }
                }

                pointer.take("Identifier");

                let columnType = pointer.take("TableType").unwrap().tokenValue();
                let isOptional = pointer.toEqual("Punctuation", "?");

                let decorators = readDecorators(pointer, columnType.clone(), &columns);
                let isPrimary = decorators.contains(&(String::from("id"), String::new()));
                let uuid = decorators.contains(&(String::from("uuid"), String::new()));
                let autoincrement =
                    decorators.contains(&(String::from("autoincrement"), String::new()));

                let default = {
                    match decorators.iter().find(|x| x.0 == "default") {
                        Some(data) => Some(data.1.clone()),
                        _ => None,
                    }
                };

                if autoincrement && columnType != "Integer" {
                    pointer.error(&format!(
                        "autoincrement can only be used with integer columns"
                    ));
                }

                let tableColumn = TableColumn {
                    columnType,
                    name,
                    isOptional,
                    isPrimary,
                    default,
                    uuid,
                    autoincrement,
                };

                columns.push(tableColumn);
            }
            _ => break,
        }
    }

    pointer
        .tokens
        .push(ParsedToken::Table(tableName.unwrap().tokenValue(), columns));
}
