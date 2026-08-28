use tower_lsp::lsp_types::{Position, Range};

const SCALAR_TYPES: [&str; 7] = ["String", "Boolean", "Integer", "Float", "DateTime", "Date", "Json"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Config,
    Enum,
    Model,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub struct AttributeArgumentInfo {
    pub name: Symbol,
    pub values: Vec<Symbol>,
}

#[derive(Debug, Clone)]
pub struct AttributeInfo {
    pub name: Symbol,
    pub range: Range,
    pub arguments: Vec<AttributeArgumentInfo>,
}

impl AttributeInfo {
    pub fn argument(&self, name: &str) -> Option<&AttributeArgumentInfo> {
        self.arguments.iter().find(|argument| argument.name.name == name)
    }
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: Symbol,
    pub ty: Symbol,
    pub optional: bool,
    pub list: bool,
    pub range: Range,
    pub attributes: Vec<AttributeInfo>,
}

impl FieldInfo {
    pub fn attribute(&self, name: &str) -> Option<&AttributeInfo> {
        self.attributes.iter().find(|attribute| attribute.name.name == name)
    }

    pub fn display_type(&self) -> String {
        if self.list {
            format!("{}[]", self.ty.name)
        } else if self.optional {
            format!("{}?", self.ty.name)
        } else {
            self.ty.name.clone()
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlockInfo {
    pub kind: BlockKind,
    pub name: Option<Symbol>,
    pub range: Range,
    pub body_range: Range,
    pub fields: Vec<FieldInfo>,
    pub attributes: Vec<AttributeInfo>,
    pub values: Vec<Symbol>,
    pub entries: Vec<Symbol>,
}

impl BlockInfo {
    pub fn field(&self, name: &str) -> Option<&FieldInfo> {
        self.fields.iter().find(|field| field.name.name == name)
    }

    pub fn attribute(&self, name: &str) -> Option<&AttributeInfo> {
        self.attributes.iter().find(|attribute| attribute.name.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Ident,
    String,
    Number,
    Symbol,
}

#[derive(Debug, Clone)]
struct Token {
    text: String,
    kind: TokenKind,
    range: Range,
}

impl Token {
    fn is_symbol(&self, symbol: &str) -> bool {
        self.kind == TokenKind::Symbol && self.text == symbol
    }

    fn as_value_symbol(&self) -> Symbol {
        let name =
            if self.kind == TokenKind::String { self.text.trim_matches('"').to_string() } else { self.text.clone() };

        Symbol { name, range: self.range }
    }
}

#[derive(Debug, Clone)]
pub struct DocumentIndex {
    pub blocks: Vec<BlockInfo>,
    external_blocks: Vec<BlockInfo>,
    tokens: Vec<Token>,
    end: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedSymbol {
    Type(String),
    Field { model: String, field: String },
    EnumValue { enum_name: String, value: String },
}

impl DocumentIndex {
    pub fn new(source: &str) -> Self {
        let positions = LineIndex::new(source);
        let tokens = tokenize(source, &positions);
        let blocks = parse_blocks(&tokens);

        Self { blocks, external_blocks: Vec::new(), tokens, end: positions.position(source.len()) }
    }

    pub fn end_position(&self) -> Position {
        self.end
    }

    pub fn block_at(&self, position: Position) -> Option<&BlockInfo> {
        self.blocks.iter().find(|block| contains(block.range, position))
    }

    pub fn model(&self, name: &str) -> Option<&BlockInfo> {
        self.declarations().find(|block| {
            block.kind == BlockKind::Model && block.name.as_ref().is_some_and(|symbol| symbol.name == name)
        })
    }

    pub fn enum_(&self, name: &str) -> Option<&BlockInfo> {
        self.declarations().find(|block| {
            block.kind == BlockKind::Enum && block.name.as_ref().is_some_and(|symbol| symbol.name == name)
        })
    }

    pub fn config(&self) -> Option<&BlockInfo> {
        self.blocks.iter().find(|block| block.kind == BlockKind::Config)
    }

    pub fn field_at(&self, position: Position) -> Option<(&BlockInfo, &FieldInfo)> {
        let model = self.block_at(position).filter(|block| block.kind == BlockKind::Model)?;
        let field = model
            .fields
            .iter()
            .filter(|field| {
                contains(field.range, position)
                    || (field.range.start.line == position.line && field.range.start.character <= position.character)
            })
            .max_by_key(|field| (field.range.start.line, field.range.start.character))?;

        Some((model, field))
    }

    pub fn token_symbol_at(&self, position: Position) -> Option<Symbol> {
        self.tokens
            .iter()
            .filter(|token| matches!(token.kind, TokenKind::Ident | TokenKind::String))
            .find(|token| token_contains(token.range, position))
            .map(Token::as_value_symbol)
    }

    pub fn type_names(&self) -> Vec<String> {
        self.declarations()
            .filter_map(|block| match block.kind {
                BlockKind::Model | BlockKind::Enum => block.name.as_ref().map(|name| name.name.clone()),
                BlockKind::Config => None,
            })
            .collect()
    }

    pub fn with_external_declarations(&self, declarations: impl IntoIterator<Item = BlockInfo>) -> Self {
        let mut index = self.clone();
        index.external_blocks = declarations.into_iter().collect();
        index
    }

    pub fn declarations(&self) -> impl Iterator<Item = &BlockInfo> {
        self.blocks.iter().chain(&self.external_blocks)
    }

    pub fn resolve_symbol(&self, position: Position) -> Option<ResolvedSymbol> {
        let token = self.token_symbol_at(position)?;

        for block in &self.blocks {
            if block.name.as_ref().is_some_and(|name| name.range == token.range) {
                return Some(ResolvedSymbol::Type(token.name));
            }

            if block.kind == BlockKind::Enum {
                if block.values.iter().any(|value| value.range == token.range) {
                    return Some(ResolvedSymbol::EnumValue {
                        enum_name: block.name.as_ref()?.name.clone(),
                        value: token.name,
                    });
                }
                continue;
            }

            if block.kind != BlockKind::Model {
                continue;
            }

            let model_name = &block.name.as_ref()?.name;
            for field in &block.fields {
                if field.name.range == token.range {
                    return Some(ResolvedSymbol::Field { model: model_name.clone(), field: token.name });
                }
                if field.ty.range == token.range && self.is_named_type(&token.name) {
                    return Some(ResolvedSymbol::Type(token.name));
                }

                for attribute in &field.attributes {
                    if attribute.name.name == "relation" {
                        if attribute
                            .argument("fields")
                            .is_some_and(|argument| argument.values.iter().any(|value| value.range == token.range))
                        {
                            return Some(ResolvedSymbol::Field { model: model_name.clone(), field: token.name });
                        }
                        if attribute
                            .argument("references")
                            .is_some_and(|argument| argument.values.iter().any(|value| value.range == token.range))
                        {
                            return Some(ResolvedSymbol::Field { model: field.ty.name.clone(), field: token.name });
                        }
                    }

                    if attribute.name.name == "default"
                        && self.enum_(&field.ty.name).is_some()
                        && attribute
                            .arguments
                            .iter()
                            .flat_map(|argument| &argument.values)
                            .any(|value| value.range == token.range)
                    {
                        return Some(ResolvedSymbol::EnumValue { enum_name: field.ty.name.clone(), value: token.name });
                    }
                }
            }

            for attribute in &block.attributes {
                if matches!(attribute.name.name.as_str(), "ids" | "uniques" | "indexes" | "fulltexts")
                    && attribute
                        .arguments
                        .iter()
                        .flat_map(|argument| &argument.values)
                        .any(|value| value.range == token.range)
                {
                    return Some(ResolvedSymbol::Field { model: model_name.clone(), field: token.name });
                }
            }
        }

        if self.is_named_type(&token.name) { Some(ResolvedSymbol::Type(token.name)) } else { None }
    }

    pub fn definition(&self, symbol: &ResolvedSymbol) -> Option<Range> {
        match symbol {
            ResolvedSymbol::Type(name) => self
                .blocks
                .iter()
                .find_map(|block| block.name.as_ref().filter(|symbol| symbol.name == *name).map(|symbol| symbol.range)),
            ResolvedSymbol::Field { model, field } => self.model(model)?.field(field).map(|field| field.name.range),
            ResolvedSymbol::EnumValue { enum_name, value } => {
                self.enum_(enum_name)?.values.iter().find(|symbol| symbol.name == *value).map(|symbol| symbol.range)
            }
        }
    }

    pub fn occurrences(&self, target: &ResolvedSymbol) -> Vec<Range> {
        let mut ranges = Vec::new();

        if let ResolvedSymbol::Type(name) = target {
            ranges.extend(self.import_symbol_occurrences(name));
        }

        for block in &self.blocks {
            if let (ResolvedSymbol::Type(target_name), Some(name)) = (target, &block.name)
                && name.name == *target_name
            {
                ranges.push(name.range);
            }

            if let ResolvedSymbol::EnumValue { enum_name, value } = target
                && block.kind == BlockKind::Enum
                && block.name.as_ref().is_some_and(|name| name.name == *enum_name)
            {
                ranges.extend(block.values.iter().filter(|item| item.name == *value).map(|item| item.range));
            }

            if block.kind != BlockKind::Model {
                continue;
            }

            let Some(model_name) = block.name.as_ref().map(|name| name.name.as_str()) else {
                continue;
            };

            for field in &block.fields {
                if matches!(target, ResolvedSymbol::Type(name) if field.ty.name == *name) {
                    ranges.push(field.ty.range);
                }
                if matches!(target, ResolvedSymbol::Field { model, field: name } if model == model_name && field.name.name == *name)
                {
                    ranges.push(field.name.range);
                }

                for attribute in &field.attributes {
                    if attribute.name.name == "relation"
                        && let ResolvedSymbol::Field { model, field: target_field } = target
                    {
                        if model == model_name
                            && let Some(argument) = attribute.argument("fields")
                        {
                            ranges.extend(
                                argument
                                    .values
                                    .iter()
                                    .filter(|value| value.name == *target_field)
                                    .map(|value| value.range),
                            );
                        }
                        if model == &field.ty.name
                            && let Some(argument) = attribute.argument("references")
                        {
                            ranges.extend(
                                argument
                                    .values
                                    .iter()
                                    .filter(|value| value.name == *target_field)
                                    .map(|value| value.range),
                            );
                        }
                    }

                    if let ResolvedSymbol::EnumValue { enum_name, value } = target
                        && field.ty.name == *enum_name
                        && attribute.name.name == "default"
                    {
                        ranges.extend(
                            attribute
                                .arguments
                                .iter()
                                .flat_map(|argument| &argument.values)
                                .filter(|symbol| symbol.name == *value)
                                .map(|symbol| symbol.range),
                        );
                    }
                }
            }

            if let ResolvedSymbol::Field { model, field } = target
                && model == model_name
            {
                for attribute in &block.attributes {
                    if matches!(attribute.name.name.as_str(), "ids" | "uniques" | "indexes" | "fulltexts") {
                        ranges.extend(
                            attribute
                                .arguments
                                .iter()
                                .flat_map(|argument| &argument.values)
                                .filter(|value| value.name == *field)
                                .map(|value| value.range),
                        );
                    }
                }
            }
        }

        ranges.sort_by_key(|range| (range.start.line, range.start.character));
        ranges.dedup();
        ranges
    }

    pub fn is_named_type(&self, name: &str) -> bool {
        !SCALAR_TYPES.contains(&name)
            && self.declarations().any(|block| {
                block.name.as_ref().is_some_and(|symbol| symbol.name == name)
                    && matches!(block.kind, BlockKind::Model | BlockKind::Enum)
            })
    }

    fn import_symbol_occurrences(&self, name: &str) -> Vec<Range> {
        let mut occurrences = Vec::new();
        let mut cursor = 0usize;
        while cursor < self.tokens.len() {
            if self.tokens[cursor].kind != TokenKind::Ident || self.tokens[cursor].text != "import" {
                cursor += 1;
                continue;
            }
            cursor += 1;
            if !self.tokens.get(cursor).is_some_and(|token| token.is_symbol("{")) {
                continue;
            }
            cursor += 1;
            while cursor < self.tokens.len() && !self.tokens[cursor].is_symbol("}") {
                if self.tokens[cursor].kind == TokenKind::Ident && self.tokens[cursor].text == name {
                    occurrences.push(self.tokens[cursor].range);
                }
                cursor += 1;
            }
        }
        occurrences
    }
}

pub fn scalar_types() -> &'static [&'static str] {
    &SCALAR_TYPES
}

pub fn contains(range: Range, position: Position) -> bool {
    position_le(range.start, position) && position_le(position, range.end)
}

pub fn position_le(left: Position, right: Position) -> bool {
    (left.line, left.character) <= (right.line, right.character)
}

fn token_contains(range: Range, position: Position) -> bool {
    position_le(range.start, position)
        && ((position.line, position.character) < (range.end.line, range.end.character) || position == range.start)
}

pub fn line_prefix(source: &str, position: Position) -> &str {
    let line = source.lines().nth(position.line as usize).unwrap_or_default();
    let mut utf16 = 0u32;
    let mut byte = line.len();

    for (index, character) in line.char_indices() {
        if utf16 >= position.character {
            byte = index;
            break;
        }
        utf16 += character.len_utf16() as u32;
    }

    &line[..byte]
}

#[derive(Debug)]
struct LineIndex {
    starts: Vec<usize>,
    source: String,
}

impl LineIndex {
    fn new(source: &str) -> Self {
        let mut starts = vec![0];
        for (index, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                starts.push(index + 1);
            }
        }
        Self { starts, source: source.to_string() }
    }

    fn position(&self, offset: usize) -> Position {
        let line = self.starts.partition_point(|start| *start <= offset).saturating_sub(1);
        let line_start = self.starts[line];
        let character = self.source[line_start..offset].encode_utf16().count() as u32;
        Position::new(line as u32, character)
    }
}

fn tokenize(source: &str, positions: &LineIndex) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut offset = 0usize;

    while offset < source.len() {
        let character = source[offset..].chars().next().expect("valid character boundary");

        if character.is_whitespace() {
            offset += character.len_utf8();
            continue;
        }
        if character == '#' || source[offset..].starts_with("//") {
            offset = source[offset..].find('\n').map_or(source.len(), |end| offset + end);
            continue;
        }

        let start = offset;
        let kind = if character == '"' {
            offset += character.len_utf8();
            let mut escaped = false;
            while offset < source.len() {
                let current = source[offset..].chars().next().expect("valid character boundary");
                offset += current.len_utf8();
                if current == '"' && !escaped {
                    break;
                }
                escaped = current == '\\' && !escaped;
                if current != '\\' {
                    escaped = false;
                }
            }
            TokenKind::String
        } else if character.is_ascii_alphabetic() || character == '_' {
            offset += character.len_utf8();
            while offset < source.len() {
                let current = source.as_bytes()[offset];
                if current.is_ascii_alphanumeric() || current == b'_' {
                    offset += 1;
                } else {
                    break;
                }
            }
            TokenKind::Ident
        } else if character.is_ascii_digit()
            || (character == '-'
                && source[offset + character.len_utf8()..].chars().next().is_some_and(|next| next.is_ascii_digit()))
        {
            offset += character.len_utf8();
            while offset < source.len() {
                let current = source.as_bytes()[offset];
                if current.is_ascii_digit() || current == b'.' {
                    offset += 1;
                } else {
                    break;
                }
            }
            TokenKind::Number
        } else {
            offset += character.len_utf8();
            TokenKind::Symbol
        };

        tokens.push(Token {
            text: source[start..offset].to_string(),
            kind,
            range: Range::new(positions.position(start), positions.position(offset)),
        });
    }

    tokens
}

fn parse_blocks(tokens: &[Token]) -> Vec<BlockInfo> {
    let mut blocks = Vec::new();
    let mut cursor = 0usize;

    while cursor < tokens.len() {
        let Some(kind) = block_kind(&tokens[cursor]) else {
            cursor += 1;
            continue;
        };
        let name_index = match kind {
            BlockKind::Config => None,
            BlockKind::Enum | BlockKind::Model => {
                tokens.get(cursor + 1).filter(|token| token.kind == TokenKind::Ident).map(|_| cursor + 1)
            }
        };
        let open_from = name_index.map_or(cursor + 1, |index| index + 1);
        let Some(open) = (open_from..tokens.len()).find(|index| tokens[*index].is_symbol("{")) else {
            cursor += 1;
            continue;
        };
        let close = matching_token(tokens, open, "{", "}").unwrap_or(tokens.len().saturating_sub(1));
        let body_end = close.max(open + 1);
        let (fields, attributes, values, entries) = match kind {
            BlockKind::Model => (
                parse_fields(tokens, open + 1, body_end),
                parse_model_attributes(tokens, open + 1, body_end),
                Vec::new(),
                Vec::new(),
            ),
            BlockKind::Enum => (Vec::new(), Vec::new(), parse_enum_values(tokens, open + 1, body_end), Vec::new()),
            BlockKind::Config => (Vec::new(), Vec::new(), Vec::new(), parse_config_entries(tokens, open + 1, body_end)),
        };

        blocks.push(BlockInfo {
            kind,
            name: name_index.map(|index| tokens[index].as_value_symbol()),
            range: Range::new(tokens[cursor].range.start, tokens[close].range.end),
            body_range: Range::new(tokens[open].range.end, tokens[close].range.start),
            fields,
            attributes,
            values,
            entries,
        });
        cursor = close.saturating_add(1);
    }

    blocks
}

fn parse_model_attributes(tokens: &[Token], start: usize, end: usize) -> Vec<AttributeInfo> {
    let mut attributes = Vec::new();
    let mut cursor = start;

    while cursor + 2 < end {
        if !tokens[cursor].is_symbol("@")
            || !tokens[cursor + 1].is_symbol("@")
            || tokens[cursor + 2].kind != TokenKind::Ident
        {
            cursor += 1;
            continue;
        }

        let attribute_start = cursor;
        let name_index = cursor + 2;
        cursor += 3;
        let mut arguments = Vec::new();
        let mut last = name_index;
        if cursor < end && tokens[cursor].is_symbol("(") {
            let close = matching_token(tokens, cursor, "(", ")").unwrap_or(end.saturating_sub(1));
            arguments = parse_attribute_arguments(tokens, cursor + 1, close);
            cursor = close.saturating_add(1);
            last = close;
        }

        attributes.push(AttributeInfo {
            name: tokens[name_index].as_value_symbol(),
            range: Range::new(tokens[attribute_start].range.start, tokens[last].range.end),
            arguments,
        });
    }

    attributes
}

fn block_kind(token: &Token) -> Option<BlockKind> {
    if token.kind != TokenKind::Ident {
        return None;
    }
    match token.text.as_str() {
        "config" => Some(BlockKind::Config),
        "enum" => Some(BlockKind::Enum),
        "model" => Some(BlockKind::Model),
        _ => None,
    }
}

fn parse_fields(tokens: &[Token], start: usize, end: usize) -> Vec<FieldInfo> {
    let mut fields = Vec::new();
    let mut cursor = start;

    while cursor + 1 < end {
        if tokens[cursor].kind != TokenKind::Ident || tokens[cursor + 1].kind != TokenKind::Ident {
            cursor += 1;
            continue;
        }

        let name = tokens[cursor].as_value_symbol();
        let ty = tokens[cursor + 1].as_value_symbol();
        let mut optional = false;
        let mut list = false;
        let mut attributes = Vec::new();
        let mut next = cursor + 2;
        let mut last = cursor + 1;

        if next < end && tokens[next].is_symbol("?") {
            optional = true;
            last = next;
            next += 1;
        }
        if next + 1 < end && tokens[next].is_symbol("[") && tokens[next + 1].is_symbol("]") {
            list = true;
            optional = false;
            last = next + 1;
            next += 2;
        }

        while next + 1 < end && tokens[next].is_symbol("@") && tokens[next + 1].kind == TokenKind::Ident {
            let attribute_start = next;
            let name_index = next + 1;
            next += 2;
            let mut arguments = Vec::new();

            if next < end && tokens[next].is_symbol("(") {
                let close = matching_token(tokens, next, "(", ")").unwrap_or_else(|| {
                    (next + 1..end)
                        .find(|index| tokens[*index].is_symbol("@"))
                        .map_or(end.saturating_sub(1), |index| index.saturating_sub(1))
                });
                arguments = parse_attribute_arguments(tokens, next + 1, close);
                next = close.saturating_add(1);
                last = close;
            } else {
                last = name_index;
            }

            attributes.push(AttributeInfo {
                name: tokens[name_index].as_value_symbol(),
                range: Range::new(tokens[attribute_start].range.start, tokens[last].range.end),
                arguments,
            });
        }

        fields.push(FieldInfo {
            name,
            ty,
            optional,
            list,
            range: Range::new(tokens[cursor].range.start, tokens[last].range.end),
            attributes,
        });
        cursor = next.max(cursor + 2);
    }

    fields
}

fn parse_attribute_arguments(tokens: &[Token], start: usize, end: usize) -> Vec<AttributeArgumentInfo> {
    let mut arguments = Vec::new();
    let mut cursor = start;

    while cursor < end {
        if tokens[cursor].kind != TokenKind::Ident
            || cursor + 1 >= end
            || !(tokens[cursor + 1].is_symbol(":") || tokens[cursor + 1].is_symbol("="))
        {
            cursor += 1;
            continue;
        }

        let name = tokens[cursor].as_value_symbol();
        let value_start = cursor + 2;
        let mut value_end = value_start;
        let mut depth = 0i32;
        while value_end < end {
            if matches!(tokens[value_end].text.as_str(), "[" | "(") {
                depth += 1;
            } else if matches!(tokens[value_end].text.as_str(), "]" | ")") {
                depth -= 1;
            } else if tokens[value_end].is_symbol(",") && depth == 0 {
                break;
            }
            value_end += 1;
        }

        let values = tokens[value_start..value_end]
            .iter()
            .filter(|token| matches!(token.kind, TokenKind::Ident | TokenKind::String | TokenKind::Number))
            .map(Token::as_value_symbol)
            .collect();
        arguments.push(AttributeArgumentInfo { name, values });
        cursor = value_end.saturating_add(1);
    }

    if arguments.is_empty() && start < end {
        let values = tokens[start..end]
            .iter()
            .filter(|token| matches!(token.kind, TokenKind::Ident | TokenKind::String | TokenKind::Number))
            .map(Token::as_value_symbol)
            .collect();
        arguments
            .push(AttributeArgumentInfo { name: Symbol { name: String::new(), range: tokens[start].range }, values });
    }

    arguments
}

fn parse_enum_values(tokens: &[Token], start: usize, end: usize) -> Vec<Symbol> {
    tokens[start..end].iter().filter(|token| token.kind == TokenKind::Ident).map(Token::as_value_symbol).collect()
}

fn parse_config_entries(tokens: &[Token], start: usize, end: usize) -> Vec<Symbol> {
    let mut entries = Vec::new();
    let mut depth = 0i32;

    for index in start..end {
        if matches!(tokens[index].text.as_str(), "[" | "(") {
            depth += 1;
            continue;
        }
        if matches!(tokens[index].text.as_str(), "]" | ")") {
            depth -= 1;
            continue;
        }
        if depth == 0
            && tokens[index].kind == TokenKind::Ident
            && tokens.get(index + 1).is_some_and(|token| token.is_symbol("="))
        {
            entries.push(tokens[index].as_value_symbol());
        }
    }

    entries
}

fn matching_token(tokens: &[Token], open: usize, opening: &str, closing: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if token.is_symbol(opening) {
            depth += 1;
        } else if token.is_symbol(closing) {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = r#"config {
    database = "postgresql"
    database_url = env("DATABASE_URL")
}

enum Role { USER ADMIN }

model User {
    id String @id @default(uuid())
    manager User? @relation(name: "management", fields: [manager_id], references: [id])
    manager_id String?
    role Role @default(USER)
}
"#;

    #[test]
    fn indexes_blocks_fields_and_attributes() {
        let index = DocumentIndex::new(SCHEMA);
        assert_eq!(index.blocks.len(), 3);
        let user = index.model("User").expect("user model");
        assert_eq!(user.fields.len(), 4);
        let manager = user.field("manager").expect("manager");
        assert!(manager.optional);
        let relation = manager.attribute("relation").expect("relation");
        assert_eq!(relation.argument("fields").expect("fields").values[0].name, "manager_id");
    }

    #[test]
    fn resolves_relation_and_enum_references() {
        let index = DocumentIndex::new(SCHEMA);
        let user = index.model("User").expect("user");
        let manager = user.field("manager").expect("manager");
        let reference =
            manager.attribute("relation").expect("relation").argument("references").expect("references").values[0]
                .range;
        assert_eq!(
            index.resolve_symbol(reference.start),
            Some(ResolvedSymbol::Field { model: "User".into(), field: "id".into() })
        );

        let role = user.field("role").expect("role");
        let value = role.attribute("default").expect("default").arguments[0].values[0].range;
        assert_eq!(
            index.resolve_symbol(value.start),
            Some(ResolvedSymbol::EnumValue { enum_name: "Role".into(), value: "USER".into() })
        );
    }

    #[test]
    fn type_occurrences_include_named_imports_without_mixing_external_ranges() {
        let source = "import { Account } from \"account.dinoco\"\nmodel Session { id String @id account Account }";
        let local = DocumentIndex::new(source);
        let imported = DocumentIndex::new("model Account { id String @id }");
        let index = local.with_external_declarations(imported.blocks);
        let account_type = index.model("Session").expect("session").field("account").expect("account").ty.range;

        assert_eq!(index.resolve_symbol(account_type.start), Some(ResolvedSymbol::Type("Account".to_string())));
        assert_eq!(index.occurrences(&ResolvedSymbol::Type("Account".to_string())).len(), 2);
    }
}
