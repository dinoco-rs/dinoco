use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::document::{BlockInfo, BlockKind, DocumentIndex, ResolvedSymbol};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportSpec {
    pub symbols: Vec<String>,
    pub path: String,
}

#[derive(Debug)]
pub struct SchemaFile {
    pub source: String,
    pub index: DocumentIndex,
    imports: Vec<ImportSpec>,
}

impl SchemaFile {
    pub fn new(source: String) -> Self {
        let index = DocumentIndex::new(&source);
        let imports = extract_imports(&source);
        Self { source, index, imports }
    }

    pub fn imports(&self) -> &[ImportSpec] {
        &self.imports
    }
}

#[derive(Debug, Clone)]
pub struct ImportEdge {
    pub import: ImportSpec,
    pub target: PathBuf,
}

#[derive(Debug, Default)]
pub struct ImportGraph {
    pub files: HashMap<PathBuf, Arc<SchemaFile>>,
    pub edges: HashMap<PathBuf, Vec<ImportEdge>>,
}

impl ImportGraph {
    pub fn visible_declarations(&self, file: &Path) -> Vec<BlockInfo> {
        self.edges
            .get(file)
            .into_iter()
            .flatten()
            .filter_map(|edge| self.files.get(&edge.target).map(|target| (edge, target)))
            .flat_map(|(edge, target)| {
                target.index.blocks.iter().filter_map(move |block| {
                    let name = block.name.as_ref()?;
                    if !matches!(block.kind, BlockKind::Model | BlockKind::Enum) {
                        return None;
                    }
                    if edge.import.symbols.is_empty() || edge.import.symbols.contains(&name.name) {
                        Some(block.clone())
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    pub fn declaration(&self, file: &Path, name: &str) -> Option<(PathBuf, &BlockInfo)> {
        if let Some(local) = self.files.get(file).and_then(|item| {
            item.index.blocks.iter().find(|block| block.name.as_ref().is_some_and(|item| item.name == name))
        }) {
            return Some((file.to_path_buf(), local));
        }

        for edge in self.edges.get(file)? {
            if !edge.import.symbols.is_empty() && !edge.import.symbols.iter().any(|symbol| symbol == name) {
                continue;
            }
            let target = self.files.get(&edge.target)?;
            if let Some(block) =
                target.index.blocks.iter().find(|block| block.name.as_ref().is_some_and(|item| item.name == name))
            {
                return Some((edge.target.clone(), block));
            }
        }
        None
    }

    pub fn definition(&self, file: &Path, symbol: &ResolvedSymbol) -> Option<(PathBuf, tower_lsp::lsp_types::Range)> {
        match symbol {
            ResolvedSymbol::Type(name) => {
                let (path, block) = self.declaration(file, name)?;
                Some((path, block.name.as_ref()?.range))
            }
            ResolvedSymbol::Field { model, field } => {
                let (path, block) = self.declaration(file, model)?;
                Some((path, block.field(field)?.name.range))
            }
            ResolvedSymbol::EnumValue { enum_name, value } => {
                let (path, block) = self.declaration(file, enum_name)?;
                Some((path, block.values.iter().find(|item| item.name == *value)?.range))
            }
        }
    }
}

#[derive(Debug)]
struct CachedFile {
    modified: Option<SystemTime>,
    len: u64,
    file: Arc<SchemaFile>,
    last_seen: u64,
}

#[derive(Debug, Default)]
pub struct WorkspaceCache {
    files: HashMap<PathBuf, CachedFile>,
    known_roots: HashMap<PathBuf, HashSet<PathBuf>>,
    generation: u64,
}

impl WorkspaceCache {
    pub fn load_graph(&mut self, entry: &Path, overlays: &HashMap<PathBuf, Arc<SchemaFile>>) -> ImportGraph {
        self.generation = self.generation.wrapping_add(1);
        let Some(entry) = canonical_path(entry) else {
            return ImportGraph::default();
        };
        let mut graph = ImportGraph::default();
        let mut pending = vec![entry.clone()];
        let mut visited = HashSet::new();

        while let Some(path) = pending.pop() {
            if !visited.insert(path.clone()) {
                continue;
            }
            let Some(file) = overlays.get(&path).cloned().or_else(|| self.load_disk_file(&path)) else {
                continue;
            };
            let mut edges = Vec::new();
            for import in file.imports() {
                let Some(target) = resolve_import_path(&path, &import.path) else {
                    continue;
                };
                edges.push(ImportEdge { import: import.clone(), target: target.clone() });
                if !visited.contains(&target) {
                    pending.push(target);
                }
            }
            graph.files.insert(path.clone(), file);
            graph.edges.insert(path, edges);
        }

        if entry.file_name().and_then(|name| name.to_str()) == Some("schema.dinoco") {
            self.known_roots.insert(entry, visited);
        }
        self.prune_stale_entries();
        graph
    }

    pub fn load_import_target(
        &mut self,
        current: &Path,
        import_path: &str,
        overlays: &HashMap<PathBuf, Arc<SchemaFile>>,
    ) -> Option<(PathBuf, Arc<SchemaFile>)> {
        let current = canonical_path(current)?;
        let target = resolve_import_path(&current, import_path)?;
        let file = overlays.get(&target).cloned().or_else(|| self.load_disk_file(&target))?;
        Some((target, file))
    }

    pub fn known_root_for(&self, file: &Path) -> Option<PathBuf> {
        let file = canonical_path(file)?;
        self.known_roots
            .iter()
            .filter(|(_, files)| files.contains(&file))
            .map(|(root, _)| root.clone())
            .max_by_key(|root| root.components().count())
    }

    pub fn invalidate(&mut self, path: &Path) {
        if let Some(path) = canonical_path(path) {
            self.files.remove(&path);
        }
    }

    fn load_disk_file(&mut self, path: &Path) -> Option<Arc<SchemaFile>> {
        let metadata = fs::metadata(path).ok()?;
        let modified = metadata.modified().ok();
        if let Some(cached) = self.files.get_mut(path)
            && cached.modified == modified
            && cached.len == metadata.len()
        {
            cached.last_seen = self.generation;
            return Some(cached.file.clone());
        }

        let source = fs::read_to_string(path).ok()?;
        let file = Arc::new(SchemaFile::new(source));
        self.files.insert(
            path.to_path_buf(),
            CachedFile { modified, len: metadata.len(), file: file.clone(), last_seen: self.generation },
        );
        Some(file)
    }

    fn prune_stale_entries(&mut self) {
        if self.files.len() <= 1024 {
            return;
        }
        let oldest = self.generation.saturating_sub(16);
        self.files.retain(|_, file| file.last_seen >= oldest);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPathSuggestion {
    pub path: String,
    pub directory: bool,
}

pub fn import_path_suggestions(current: &Path, fragment: &str) -> Vec<ImportPathSuggestion> {
    let parent = current.parent().unwrap_or_else(|| Path::new("."));
    let fragment = fragment.replace('\\', "/");
    let (typed_directory, name_fragment) = fragment.rsplit_once('/').map_or(("", fragment.as_str()), |parts| parts);
    let display_directory = if typed_directory.is_empty() { "./".to_string() } else { format!("{typed_directory}/") };
    let disk_directory = if typed_directory.is_empty() { parent.to_path_buf() } else { parent.join(typed_directory) };
    let current = canonical_path(current);
    let mut suggestions = fs::read_dir(disk_directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            let name = entry.file_name().into_string().ok()?;
            if name.starts_with('.') || !name.starts_with(name_fragment) {
                return None;
            }
            if file_type.is_dir() {
                return Some(ImportPathSuggestion { path: format!("{display_directory}{name}/"), directory: true });
            }
            if entry.path().extension().and_then(|extension| extension.to_str()) != Some("dinoco")
                || canonical_path(&entry.path()) == current
            {
                return None;
            }
            Some(ImportPathSuggestion { path: format!("{display_directory}{name}"), directory: false })
        })
        .collect::<Vec<_>>();
    suggestions.sort_by_key(|item| (!item.directory, item.path.clone()));
    suggestions
}

pub fn canonical_path(path: &Path) -> Option<PathBuf> {
    fs::canonicalize(path).ok()
}

pub fn resolve_import_path(current: &Path, import_path: &str) -> Option<PathBuf> {
    if import_path.trim().is_empty()
        || Path::new(import_path).is_absolute()
        || Path::new(import_path).extension().and_then(|extension| extension.to_str()) != Some("dinoco")
    {
        return None;
    }
    canonical_path(&current.parent().unwrap_or_else(|| Path::new(".")).join(import_path))
}

fn extract_imports(source: &str) -> Vec<ImportSpec> {
    if let Ok(schema) = dinoco_compiler::parse(source) {
        let mut imports = schema
            .imports()
            .map(|import| ImportSpec { symbols: import.symbols.clone(), path: import.path.clone() })
            .collect::<Vec<_>>();
        imports.extend(
            schema.config_imports().map(|import| ImportSpec { symbols: Vec::new(), path: import.path.clone() }),
        );
        return imports;
    }

    source.lines().filter_map(extract_named_import).collect()
}

fn extract_named_import(line: &str) -> Option<ImportSpec> {
    let line = line.split(['#']).next()?.trim();
    let rest = line.strip_prefix("import")?.trim_start();
    let rest = rest.strip_prefix('{')?;
    let close = rest.find('}')?;
    let symbols = rest[..close]
        .split(',')
        .map(str::trim)
        .filter(|symbol| !symbol.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let from = rest[close + 1..].trim_start().strip_prefix("from")?.trim_start();
    let path = from.strip_prefix('"')?.split('"').next()?.to_string();
    Some(ImportSpec { symbols, path })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn circular_graph_loads_every_file_once_and_exposes_direct_symbols() {
        let project = tempdir().expect("project");
        let account = project.path().join("account.dinoco");
        let session = project.path().join("session.dinoco");
        fs::write(&account, "import { Session } from \"session.dinoco\"\nmodel Account { id String @id }")
            .expect("account");
        fs::write(&session, "import { Account } from \"account.dinoco\"\nmodel Session { id String @id }")
            .expect("session");

        let mut cache = WorkspaceCache::default();
        let graph = cache.load_graph(&account, &HashMap::new());
        let account = canonical_path(&account).expect("canonical account");
        let cached_account = graph.files[&account].clone();

        assert_eq!(graph.files.len(), 2);
        assert_eq!(graph.visible_declarations(&account).len(), 1);
        assert_eq!(graph.visible_declarations(&account)[0].name.as_ref().expect("name").name, "Session");

        let second_graph = cache.load_graph(&account, &HashMap::new());
        assert!(Arc::ptr_eq(&cached_account, &second_graph.files[&account]));
    }

    #[test]
    fn suggests_only_schema_files_and_directories_for_import_paths() {
        let project = tempdir().expect("project");
        let current = project.path().join("schema.dinoco");
        fs::write(&current, "").expect("schema");
        fs::write(project.path().join("account.dinoco"), "model Account { id String @id }").expect("account");
        fs::write(project.path().join("notes.txt"), "ignored").expect("notes");
        fs::create_dir(project.path().join("entities")).expect("entities");

        let suggestions = import_path_suggestions(&current, "");

        assert!(suggestions.iter().any(|item| item.path == "./account.dinoco"));
        assert!(suggestions.iter().any(|item| item.path == "./entities/"));
        assert!(!suggestions.iter().any(|item| item.path.ends_with("notes.txt")));
        assert!(!suggestions.iter().any(|item| item.path == "./schema.dinoco"));
    }

    #[test]
    fn import_target_symbols_are_available_only_for_a_valid_dinoco_path() {
        let project = tempdir().expect("project");
        let current = project.path().join("schema.dinoco");
        let target = project.path().join("entities.dinoco");
        fs::write(&current, "").expect("schema");
        fs::write(&target, "model Account { id String @id }\nenum Role { ADMIN }").expect("entities");
        let mut cache = WorkspaceCache::default();

        let (_, file) =
            cache.load_import_target(&current, "./entities.dinoco", &HashMap::new()).expect("valid import target");

        assert!(file.index.model("Account").is_some());
        assert!(file.index.enum_("Role").is_some());
        assert!(cache.load_import_target(&current, "./missing.dinoco", &HashMap::new()).is_none());
        assert!(cache.load_import_target(&current, "./entities.txt", &HashMap::new()).is_none());
    }
}
