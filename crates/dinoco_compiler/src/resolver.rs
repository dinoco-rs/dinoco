use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::{CompileError, CompileResult, Import, Schema, SourceOrigin, parser};

const SCALAR_TYPES: [&str; 7] = ["String", "Boolean", "Integer", "Float", "DateTime", "Date", "Json"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Complete,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SymbolKind {
    Model,
    Enum,
}

struct Resolver {
    root_dir: PathBuf,
    states: HashMap<PathBuf, VisitState>,
    schemas: HashMap<PathBuf, Schema>,
    order: Vec<PathBuf>,
}

pub(crate) fn compile_file(path: &Path) -> CompileResult<Schema> {
    if path.file_name().and_then(|name| name.to_str()) != Some("schema.dinoco") {
        return Err(CompileError::new("The main schema file must be named `schema.dinoco`", 1, 1)
            .with_file(path.display().to_string()));
    }
    let canonical = fs::canonicalize(path).map_err(|error| {
        CompileError::new(format!("Could not read main schema `{}`: {error}", path.display()), 1, 1)
            .with_file(path.display().to_string())
    })?;
    let root_dir = canonical.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();
    let mut resolver = Resolver { root_dir, states: HashMap::new(), schemas: HashMap::new(), order: Vec::new() };
    resolver.load(canonical.clone(), None, true)?;
    resolver.validate_duplicate_symbols()?;

    let mut items = Vec::new();
    for file in &resolver.order {
        let schema = resolver.schemas.get(file).expect("resolved schema");
        items.extend(schema.items.iter().cloned());
    }
    let schema = Schema { items };
    parser::validate_schema(&schema)?;
    Ok(schema)
}

impl Resolver {
    fn load(&mut self, canonical: PathBuf, via: Option<&Import>, root: bool) -> CompileResult<()> {
        match self.states.get(&canonical) {
            // A visiting schema has already been parsed and cached. Reusing it
            // closes a circular import without traversing that file again.
            Some(VisitState::Complete | VisitState::Visiting) => return Ok(()),
            None => {}
        }

        let display = self.display_path(&canonical);
        let source = fs::read_to_string(&canonical).map_err(|error| {
            via.map(|import| {
                CompileError::at(
                    format!("Imported schema `{}` could not be read: {error}", import.path),
                    &import.origin,
                )
            })
            .unwrap_or_else(|| CompileError::new(format!("Could not read schema: {error}"), 1, 1).with_file(&display))
        })?;
        let schema = parser::parse_schema_with_file(&source, &display)?;
        if !root && let Some(config) = schema.config() {
            return Err(CompileError::at("Only `schema.dinoco` may declare a `config` block", &config.origin));
        }

        self.validate_import_declarations(&schema)?;
        self.states.insert(canonical.clone(), VisitState::Visiting);
        self.schemas.insert(canonical.clone(), schema);

        let mut imports = self.schemas[&canonical].imports().cloned().collect::<Vec<_>>();
        if root {
            imports.extend(self.schemas[&canonical].config_imports().map(|import| Import {
                symbols: Vec::new(),
                path: import.path.clone(),
                origin: import.origin.clone(),
            }));
        }
        let mut resolved_imports = Vec::new();
        let mut imported_files = HashMap::<PathBuf, SourceOrigin>::new();
        for import in imports {
            let imported_path = self.resolve_import_path(&canonical, &import)?;
            if let Some(first) = imported_files.insert(imported_path.clone(), import.origin.clone()) {
                return Err(CompileError::at(
                    format!("Schema `{}` is imported more than once in this file", self.display_path(&imported_path)),
                    &import.origin,
                )
                .with_related("first import is here", &first));
            }
            self.load(imported_path.clone(), Some(&import), false)?;
            self.validate_imported_symbols(&import, &imported_path)?;
            resolved_imports.push((import, imported_path));
        }
        self.validate_file_scope(&canonical, &resolved_imports)?;

        self.states.insert(canonical.clone(), VisitState::Complete);
        self.order.push(canonical);
        Ok(())
    }

    fn resolve_import_path(&self, current: &Path, import: &Import) -> CompileResult<PathBuf> {
        if import.path.trim().is_empty() {
            return Err(CompileError::at("Import path cannot be empty", &import.origin));
        }
        let path = Path::new(&import.path);
        if path.is_absolute() {
            return Err(CompileError::at("Import paths must be relative to the declaring file", &import.origin));
        }
        if path.extension().and_then(|extension| extension.to_str()) != Some("dinoco") {
            return Err(CompileError::at("Imported schema paths must end in `.dinoco`", &import.origin));
        }
        let joined = current.parent().unwrap_or_else(|| Path::new(".")).join(path);
        fs::canonicalize(&joined)
            .map_err(|_| CompileError::at(format!("Import file `{}` was not found.", import.path), &import.origin))
    }

    fn validate_import_declarations(&self, schema: &Schema) -> CompileResult<()> {
        let mut imported_names = HashMap::<&str, &SourceOrigin>::new();
        let local_names = schema
            .models()
            .map(|model| (model.name.as_str(), &model.origin))
            .chain(schema.enums().map(|item| (item.name.as_str(), &item.origin)))
            .collect::<HashMap<_, _>>();
        for import in schema.imports() {
            let mut in_statement = HashSet::new();
            for symbol in &import.symbols {
                if !in_statement.insert(symbol.as_str()) {
                    return Err(CompileError::at(
                        format!("Import `{symbol}` is listed more than once"),
                        &import.origin,
                    ));
                }
                if let Some(local) = local_names.get(symbol.as_str()) {
                    return Err(CompileError::at(
                        format!("Imported symbol `{symbol}` conflicts with a declaration in the same file"),
                        &import.origin,
                    )
                    .with_related("local declaration is here", local));
                }
                if let Some(first) = imported_names.insert(symbol, &import.origin) {
                    return Err(CompileError::at(
                        format!("Symbol `{symbol}` is imported more than once in this file"),
                        &import.origin,
                    )
                    .with_related("first import is here", first));
                }
            }
        }
        Ok(())
    }

    fn validate_imported_symbols(&self, import: &Import, imported_path: &Path) -> CompileResult<()> {
        let source = &self.schemas[imported_path];
        let declarations = source
            .models()
            .map(|model| model.name.as_str())
            .chain(source.enums().map(|item| item.name.as_str()))
            .collect::<HashSet<_>>();
        for symbol in &import.symbols {
            if !declarations.contains(symbol.as_str()) {
                return Err(CompileError::at(
                    format!("Imported symbol `{symbol}` was not found in `{}`.", import.path),
                    &import.origin,
                ));
            }
        }
        Ok(())
    }

    fn validate_file_scope(&self, file: &Path, imports: &[(Import, PathBuf)]) -> CompileResult<()> {
        let schema = &self.schemas[file];
        let mut visible = HashMap::<String, SymbolKind>::new();
        for model in schema.models() {
            visible.insert(model.name.clone(), SymbolKind::Model);
        }
        for item in schema.enums() {
            visible.insert(item.name.clone(), SymbolKind::Enum);
        }
        for (import, path) in imports {
            let source = &self.schemas[path];
            if import.symbols.is_empty() {
                for model in source.models() {
                    visible.insert(model.name.clone(), SymbolKind::Model);
                }
                for item in source.enums() {
                    visible.insert(item.name.clone(), SymbolKind::Enum);
                }
                continue;
            }
            for symbol in &import.symbols {
                let kind = if source.models().any(|model| model.name == *symbol) {
                    SymbolKind::Model
                } else {
                    SymbolKind::Enum
                };
                visible.insert(symbol.clone(), kind);
            }
        }

        for model in schema.models() {
            for field in &model.fields {
                let kind = visible.get(&field.ty.name).copied();
                if !SCALAR_TYPES.contains(&field.ty.name.as_str()) && kind.is_none() {
                    if field.ty.list || field.attributes.iter().any(|attribute| attribute.name == "relation") {
                        return Err(CompileError::at(
                            format!(
                                "Relation `{}.{}` references unknown model `{}`",
                                model.name, field.name, field.ty.name
                            ),
                            &field.origin,
                        ));
                    }
                    return Err(CompileError::at(
                        format!(
                            "Field `{}.{}` uses `{}`, which is neither declared nor imported in this file",
                            model.name, field.name, field.ty.name
                        ),
                        &field.origin,
                    ));
                }
                if field.ty.list && kind != Some(SymbolKind::Model) {
                    return Err(CompileError::at(
                        format!("Relation `{}.{}` requires model `{}` in scope", model.name, field.name, field.ty.name),
                        &field.origin,
                    ));
                }
                if field.attributes.iter().any(|attribute| attribute.name == "relation")
                    && kind != Some(SymbolKind::Model)
                {
                    return Err(CompileError::at(
                        format!(
                            "Relation `{}.{}` references unknown model `{}`",
                            model.name, field.name, field.ty.name
                        ),
                        &field.origin,
                    ));
                }
            }
        }
        Ok(())
    }

    fn validate_duplicate_symbols(&self) -> CompileResult<()> {
        let mut declarations = HashMap::<&str, &SourceOrigin>::new();
        for file in &self.order {
            let schema = &self.schemas[file];
            for (name, origin) in schema
                .models()
                .map(|model| (model.name.as_str(), &model.origin))
                .chain(schema.enums().map(|item| (item.name.as_str(), &item.origin)))
            {
                if let Some(first) = declarations.insert(name, origin) {
                    return Err(CompileError::at(format!("Symbol `{name}` is declared more than once"), origin)
                        .with_related("first declaration is here", first));
                }
            }
        }
        Ok(())
    }

    fn display_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.root_dir).unwrap_or(path).to_string_lossy().replace('\\', "/")
    }
}
