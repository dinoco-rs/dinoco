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
    Config(ConfigBlock),
    Enum(EnumDef),
    Model(Model),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigBlock {
    pub entries: Vec<ConfigEntry>,
    pub workspaces: Vec<WorkspaceConfig>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceConfig {
    pub name: String,
    pub entries: Vec<ConfigEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfigEntry {
    pub key: String,
    pub value: ConfigValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    String(String),
    Env(String),
    Array(Vec<ConfigValue>),
    Boolean(bool),
    Integer(i64),
    Ident(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDef {
    pub name: String,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub name: String,
    pub fields: Vec<ModelField>,
    pub attributes: Vec<Attribute>,
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
}

impl Attribute {
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
