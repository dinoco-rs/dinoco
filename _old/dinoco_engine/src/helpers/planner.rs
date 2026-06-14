use dinoco_compiler::ParsedFieldType;

pub fn is_destructive_cast(old_type: &ParsedFieldType, new_type: &ParsedFieldType) -> bool {
    match (old_type, new_type) {
        (ParsedFieldType::Integer, ParsedFieldType::Float) => false,
        (ParsedFieldType::Integer, ParsedFieldType::String) => false,
        (ParsedFieldType::Float, ParsedFieldType::String) => false,

        (a, b) if a == b => false,

        _ => true,
    }
}
