use dinoco_compiler::ParsedSchema;

pub fn encode_schema(mut schema: ParsedSchema) -> Vec<u8> {
    normalize_schema(&mut schema);

    bincode::serialize(&schema).expect("failed to encode schema")
}

pub fn decode_schema(bytes: &[u8]) -> ParsedSchema {
    bincode::deserialize(bytes).expect("failed to decode schema")
}

pub fn normalize_schema(schema: &mut ParsedSchema) {
    schema.tables.sort_by(|a, b| a.name.cmp(&b.name));

    for table in &mut schema.tables {
        table.fields.sort_by(|a, b| a.name.cmp(&b.name));
    }
}
