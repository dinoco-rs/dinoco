#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceOrigin {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

impl SourceOrigin {
    pub fn new(file: impl Into<String>, line: usize, column: usize) -> Self {
        Self { file: file.into(), line, column }
    }
}

impl Default for SourceOrigin {
    fn default() -> Self {
        Self::new("schema.dinoco", 1, 1)
    }
}

/// Longest identifier a database keeps. PostgreSQL silently truncates longer
/// names to 63 bytes (NAMEDATALEN - 1); MySQL rejects names over 64 characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentifierLimit {
    Bytes(usize),
    Chars(usize),
}

impl IdentifierLimit {
    pub const POSTGRES: Self = Self::Bytes(63);
    pub const MYSQL: Self = Self::Chars(64);

    /// Cuts `name` to the limit on a character boundary, the way PostgreSQL
    /// truncates identifiers, so a generated name matches the stored one.
    pub fn fit(self, mut name: String) -> String {
        let end = match self {
            Self::Bytes(max) if name.len() > max => {
                let mut end = max;
                while !name.is_char_boundary(end) {
                    end -= 1;
                }
                end
            }
            Self::Chars(max) => match name.char_indices().nth(max) {
                Some((end, _)) => end,
                None => return name,
            },
            Self::Bytes(_) => return name,
        };
        name.truncate(end);
        name
    }

    /// The limit left after reserving `len` ASCII bytes for a suffix.
    pub fn reserve(self, len: usize) -> Self {
        match self {
            Self::Bytes(max) => Self::Bytes(max.saturating_sub(len)),
            Self::Chars(max) => Self::Chars(max.saturating_sub(len)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    pub items: Vec<SchemaItem>,
}

impl Schema {
    pub fn config(&self) -> Option<&ConfigBlock> {
        self.items.iter().find_map(|item| match item {
            SchemaItem::Config(config) => Some(config),
            _ => None,
        })
    }

    pub fn models(&self) -> impl Iterator<Item = &Model> {
        self.items.iter().filter_map(|item| match item {
            SchemaItem::Model(model) => Some(model),
            _ => None,
        })
    }

    pub fn enums(&self) -> impl Iterator<Item = &EnumDef> {
        self.items.iter().filter_map(|item| match item {
            SchemaItem::Enum(def) => Some(def),
            _ => None,
        })
    }

    pub fn imports(&self) -> impl Iterator<Item = &Import> {
        self.items.iter().filter_map(|item| match item {
            SchemaItem::Import(import) => Some(import),
            _ => None,
        })
    }

    /// Longest identifier the configured database keeps, or `None` when it
    /// has no limit (SQLite) or no database is configured.
    pub fn identifier_limit(&self) -> Option<IdentifierLimit> {
        let database = self.config()?.entries.iter().find(|entry| entry.key == "database")?;
        match &database.value {
            ConfigValue::String(value) | ConfigValue::Ident(value) => match value.as_str() {
                "postgresql" | "postgres" => Some(IdentifierLimit::POSTGRES),
                "mysql" => Some(IdentifierLimit::MYSQL),
                _ => None,
            },
            _ => None,
        }
    }

    pub fn config_imports(&self) -> impl Iterator<Item = &ConfigImport> {
        self.config().into_iter().flat_map(|config| config.imports.iter())
    }

    pub fn workspaces(&self) -> impl Iterator<Item = &WorkspaceConfig> {
        self.config().into_iter().flat_map(|config| config.workspaces.iter())
    }

    pub fn workspace(&self, name: &str) -> Option<&WorkspaceConfig> {
        self.workspaces().find(|workspace| workspace.name == name)
    }

    /// Returns a schema whose effective config is the selected workspace.
    ///
    /// Models and enums are shared by every workspace. Consumers that operate on
    /// one database at a time can use this method and continue reading
    /// `Schema::config().entries` as before.
    pub fn for_workspace(&self, name: &str) -> Option<Self> {
        let entries = self.workspace(name)?.entries.clone();
        let mut schema = self.clone();
        let config = schema.items.iter_mut().find_map(|item| match item {
            SchemaItem::Config(config) => Some(config),
            _ => None,
        })?;
        config.entries = entries;
        config.workspaces.clear();
        Some(schema)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchemaItem {
    Import(Import),
    Config(ConfigBlock),
    Enum(EnumDef),
    Model(Model),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Import {
    pub symbols: Vec<String>,
    pub path: String,
    pub origin: SourceOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigBlock {
    pub entries: Vec<ConfigEntry>,
    pub workspaces: Vec<WorkspaceConfig>,
    pub imports: Vec<ConfigImport>,
    pub origin: SourceOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigImport {
    pub path: String,
    pub origin: SourceOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceConfig {
    pub name: String,
    pub entries: Vec<ConfigEntry>,
    pub origin: SourceOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigEntry {
    pub key: String,
    pub value: ConfigValue,
    pub origin: SourceOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    String(String),
    Env(String),
    Array(Vec<ConfigValue>),
    Object(Vec<ConfigEntry>),
    Boolean(bool),
    Integer(i64),
    Ident(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub values: Vec<String>,
    pub origin: SourceOrigin,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub fields: Vec<ModelField>,
    pub attributes: Vec<Attribute>,
    pub origin: SourceOrigin,
}

impl Model {
    pub fn attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes.iter().find(|attribute| attribute.name == name)
    }

    pub fn attributes(&self, name: &str) -> impl Iterator<Item = &Attribute> {
        self.attributes.iter().filter(move |attribute| attribute.name == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelField {
    pub name: String,
    pub ty: FieldType,
    pub attributes: Vec<Attribute>,
    pub origin: SourceOrigin,
}

impl ModelField {
    pub fn is_relation(&self, schema: &Schema) -> bool {
        schema.models().any(|model| model.name == self.ty.name)
    }

    pub fn is_scalar(&self, schema: &Schema) -> bool {
        !self.is_relation(schema) && !schema.enums().any(|item| item.name == self.ty.name)
    }

    pub fn is_enum(&self, schema: &Schema) -> bool {
        schema.enums().any(|item| item.name == self.ty.name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldType {
    pub name: String,
    pub optional: bool,
    pub list: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Attribute {
    pub name: String,
    pub arguments: Vec<AttributeArgument>,
    /// The `<...>` of a partial index: `@unique<email != null>`,
    /// `@@indexes<archived == false>([...])`.
    pub condition: Option<Condition>,
    pub origin: SourceOrigin,
}

impl Attribute {
    /// Whether this is an unconditional `@unique` or `@@uniques`: a partial
    /// unique index only constrains the rows its condition matches, so it is
    /// no key for relations.
    pub fn is_unconditional(&self) -> bool {
        self.condition.is_none()
    }

    pub fn argument(&self, name: &str) -> Option<&AttributeValue> {
        self.arguments.iter().find_map(|argument| match argument {
            AttributeArgument::Named { key, value } if key == name => Some(value),
            _ => None,
        })
    }

    pub fn field_names(&self) -> Option<Vec<&str>> {
        let [AttributeArgument::Value(AttributeValue::Array(values))] = self.arguments.as_slice() else {
            return None;
        };

        values
            .iter()
            .map(|value| match value {
                AttributeValue::Ident(name) => Some(name.as_str()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttributeArgument {
    Named { key: String, value: AttributeValue },
    Value(AttributeValue),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AttributeValue {
    String(String),
    Ident(String),
    Call { name: String, arguments: Vec<AttributeArgument> },
    Array(Vec<AttributeValue>),
}

/// The condition of a partial index, over the fields of its model.
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    /// `field <operator> value`.
    Compare {
        field: String,
        operator: ConditionOperator,
        value: ConditionValue,
    },
    /// A bare Boolean field, true for the rows where it is `true`.
    Field(String),
    Not(Box<Condition>),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionOperator {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
}

impl ConditionOperator {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Eq => "==",
            Self::Neq => "!=",
            Self::Lt => "<",
            Self::Lte => "<=",
            Self::Gt => ">",
            Self::Gte => ">=",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConditionValue {
    Null,
    Boolean(bool),
    /// The literal as written, so `1.50` stays `1.50`.
    Number(String),
    String(String),
    /// An enum value of the compared field.
    Ident(String),
}

impl Condition {
    /// Every field the condition reads, in order of appearance.
    pub fn fields(&self) -> Vec<&str> {
        let mut fields = Vec::new();
        self.collect_fields(&mut fields);
        fields
    }

    fn collect_fields<'a>(&'a self, fields: &mut Vec<&'a str>) {
        match self {
            Self::Compare { field, .. } | Self::Field(field) => fields.push(field),
            Self::Not(condition) => condition.collect_fields(fields),
            Self::And(conditions) | Self::Or(conditions) => {
                conditions.iter().for_each(|condition| condition.collect_fields(fields))
            }
        }
    }

    fn write(&self, out: &mut std::fmt::Formatter<'_>, parent: u8) -> std::fmt::Result {
        // Precedence: `||` 1, `&&` 2, everything else binds tighter.
        let (own, separator, conditions) = match self {
            Self::Or(conditions) => (1, " || ", conditions),
            Self::And(conditions) => (2, " && ", conditions),
            Self::Not(condition) => {
                out.write_str("!")?;
                return match condition.as_ref() {
                    Self::Field(_) | Self::Not(_) => condition.write(out, 3),
                    condition => {
                        out.write_str("(")?;
                        condition.write(out, 0)?;
                        out.write_str(")")
                    }
                };
            }
            Self::Field(field) => return out.write_str(field),
            Self::Compare { field, operator, value } => {
                write!(out, "{field} {} ", operator.as_str())?;
                return match value {
                    ConditionValue::Null => out.write_str("null"),
                    ConditionValue::Boolean(value) => write!(out, "{value}"),
                    ConditionValue::Number(value) | ConditionValue::Ident(value) => out.write_str(value),
                    ConditionValue::String(value) => {
                        out.write_str("\"")?;
                        for character in value.chars() {
                            match character {
                                '"' => out.write_str("\\\"")?,
                                '\\' => out.write_str("\\\\")?,
                                '\n' => out.write_str("\\n")?,
                                '\r' => out.write_str("\\r")?,
                                '\t' => out.write_str("\\t")?,
                                character => write!(out, "{character}")?,
                            }
                        }
                        out.write_str("\"")
                    }
                };
            }
        };

        let grouped = own < parent;
        if grouped {
            out.write_str("(")?;
        }
        for (index, condition) in conditions.iter().enumerate() {
            if index > 0 {
                out.write_str(separator)?;
            }
            condition.write(out, own + 1)?;
        }
        if grouped {
            out.write_str(")")?;
        }
        Ok(())
    }
}

/// The condition as it is written in a schema: `a != null && (b == "x" || !c)`.
impl std::fmt::Display for Condition {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.write(out, 0)
    }
}
