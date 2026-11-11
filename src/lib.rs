use chrono::{Local, NaiveDateTime};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::{Path, PathBuf}};

pub const TODOING_DIR: &str = ".todoing";
pub const TASKS_DIR: &str = "tasks";
pub const INDEX_FILE: &str = "index.json";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus { #[default] Todo, InProgress, Done, Cancelled }

impl TaskStatus {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_lowercase().as_str() {
            "todo" => Ok(Self::Todo), "in_progress" => Ok(Self::InProgress),
            "done" => Ok(Self::Done), "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("Invalid status '{}'. Valid: todo, in_progress, done, cancelled", s)),
        }
    }
    pub fn as_str(self) -> &'static str {
        match self { Self::Todo => "todo", Self::InProgress => "in_progress", Self::Done => "done", Self::Cancelled => "cancelled" }
    }
    pub fn icon(self) -> &'static str {
        match self { Self::Todo => "⏳", Self::InProgress => "🏃", Self::Done => "✅", Self::Cancelled => "❌" }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub added_at: NaiveDateTime,
    pub status: TaskStatus,
    pub labels: Vec<String>,
    pub body: String,
}

impl Task {
    pub fn new(id: i64, title: String, status: TaskStatus, labels: Vec<String>, body: String) -> Self {
        let mut seen = HashSet::new();
        let labels = labels.into_iter().filter(|s| seen.insert(s.clone())).collect();
        Self { id, title, added_at: Local::now().naive_local(), status, labels, body }
    }
    pub fn format_list(&self) -> String {
        format!("(#{}) [{}] {{{}}} {}", self.id, self.labels.join(","), self.status.icon(), self.title)
    }
    pub fn format_see(&self) -> String {
        // Match PyYAML's sorted frontmatter key order and block-list representation.
        let dt = self.added_at.format("%Y-%m-%dT%H:%M:%S").to_string();
        let labels = self.labels.iter().map(|s| yaml_scalar(s)).collect::<Vec<_>>().join(", ");
        let mut out = format!("---\\nadded_at: '{}'\\nid: {}\\nlabels: [{}]\\n", dt, self.id, labels);
        out.push_str(&format!("status: {}\\ntitle: {}\\n---\\n", self.status.as_str(), yaml_scalar(&self.title)));
        let body = self.body.trim();
        if !body.is_empty() { out.push_str("\n"); out.push_str(body); out.push('\n'); }
        out
    }
}

fn yaml_scalar(s: &str) -> String {
    if s.is_empty() { return "''".into(); }
    if s.chars().any(|c| matches!(c, '\\n' | '\\r' | ':' | '#' | '[' | ']' | '{' | '}' | ',' | '&' | '*' | '!' | '|' | '>' | '\\'' | '"' | '%' | '@' | '`'))
        || s.trim() != s || matches!(s.to_lowercase().as_str(), "null" | "true" | "false" | "yes" | "no" | "on" | "off")
    {
        format!("'{}'", s.replace('\'', "''"))
    } else { s.to_string() }
}

pub struct Store {
    root: PathBuf,
    todoing: PathBuf,
    tasks: PathBuf,
    index: PathBuf,
}

impl Store {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        let todoing = root.join(TODOING_DIR);
        let tasks = todoing.join(TASKS_DIR);
        let index = todoing.join(INDEX_FILE);
        Self { root, todoing, tasks, index }
    }
    pub fn current() -> Self { Self::new(std::env::current_dir().unwrap_or_default()) }
    pub fn ensure(&self) -> Result<(), String> {
        fs::create_dir_all(&self.tasks).map_err(|e| e.to_string())
    }
    pub fn next_id(&self) -> i64 {
        let mut max_id = 0;
        if let Ok(entries) = fs::read_dir(&self.tasks) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|x| x.to_str()) == Some("md") {
                    if let Some(stem) = p.file_stem().and_then(|x| x.to_str()) {
                        if let Ok(n) = stem.parse::<i64>() { max_id = max_id.max(n); }
                    }
                }
            }
        }
        max_id + 1
    }
    pub fn resolve_id(raw: &str) -> Result<i64, String> {
        let value = raw.trim().trim_start_matches('#');
        value.parse::<i64>().map_err(|_| format!("Invalid task ID: {}", value))
    }
    fn task_path(&self, id: i64) -> PathBuf { self.tasks.join(format!("{}.md", id)) }
    pub fn read_task(&self, id: i64) -> Result<Option<Task>, String> {
        let p = self.task_path(id);
        if !p.exists() { return Ok(None); }
        self.parse_task_file(&p)
    }
    pub fn write_task(&self, task: &Task) -> Result<(), String> {
        self.ensure()?;
        fs::write(self.task_path(task.id), task.format_see()).map_err(|e| e.to_string())
    }
    pub fn delete_task_file(&self, id: i64) -> Result<bool, String> {
        let p = self.task_path(id);
        if !p.exists() { return Ok(false); }
        fs::remove_file(p).map_err(|e| e.to_string())?; Ok(true)
    }
    pub fn read_index(&self) -> Result<Vec<Task>, String> {
        if !self.index.exists() { return self.rebuild_index(); }
        let raw = fs::read_to_string(&self.index).map_err(|e| e.to_string())?;
        match serde_json::from_str::<Vec<Task>>(&raw) { Ok(v) => Ok(v), Err(_) => self.rebuild_index() }
    }
    pub fn write_index(&self, tasks: &[Task]) -> Result<(), String> {
        self.ensure()?;
        let raw = serde_json::to_string_pretty(tasks).map_err(|e| e.to_string())?;
        fs::write(&self.index, format!("{}\n", raw)).map_err(|e| e.to_string())
    }
    pub fn rebuild_index(&self) -> Result<Vec<Task>, String> {
        self.ensure()?;
        let mut paths: Vec<PathBuf> = fs::read_dir(&self.tasks).map_err(|e| e.to_string())?
            .flatten().map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("md")).collect();
        paths.sort_by_key(|p| p.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0));
        let mut tasks = Vec::new();
        for p in paths { if let Ok(Some(t)) = self.parse_task_file(&p) { tasks.push(t); } }
        self.write_index(&tasks)?; Ok(tasks)
    }
    pub fn sync_mutation(&self, task: &Task) -> Result<(), String> {
        let mut items = self.read_index()?;
        if let Some(pos) = items.iter().position(|t| t.id == task.id) { items[pos] = task.clone(); }
        else { items.push(task.clone()); }
        self.write_index(&items)
    }
    pub fn sync_delete(&self, id: i64) -> Result<(), String> {
        let mut items = self.read_index()?;
        items.retain(|t| t.id != id);
        self.write_index(&items)
    }
    pub fn filter(&self, status: Option<TaskStatus>, labels: &[String], limit: usize, q: Option<&str>, reverse: bool) -> Result<Vec<Task>, String> {
        let mut tasks = self.read_index()?;
        if let Some(s) = status { tasks.retain(|t| t.status == s); }
        if !labels.is_empty() {
            let wanted: HashSet<String> = labels.iter().map(|x| x.to_lowercase()).collect();
            tasks.retain(|t| wanted.is_subset(&t.labels.iter().map(|x| x.to_lowercase()).collect()));
        }
        if let Some(query) = q.filter(|s| !s.is_empty()) {
            let q = query.to_lowercase();
            tasks.retain(|t| t.title.to_lowercase().contains(&q) || t.body.to_lowercase().contains(&q)
                || t.labels.join(" ").to_lowercase().contains(&q) || q == t.status.as_str());
        }
        tasks.sort_by_key(|t| t.id);
        if !reverse { tasks.reverse(); }
        if limit > 0 { tasks.truncate(limit); }
        Ok(tasks)
    }
    fn parse_task_file(&self, path: &Path) -> Result<Option<Task>, String> {
        let text = match fs::read_to_string(path) { Ok(s) => s, Err(_) => return Ok(None) };
        if !text.starts_with("---") { eprintln!("Warning: Task file {} has no frontmatter, skipping", path.display()); return Ok(None); }
        let parts: Vec<&str> = text.splitn(3, "---").collect();
        if parts.len() < 3 { return Ok(None); }
        let fm: serde_yaml::Value = match serde_yaml::from_str(parts[1]) { Ok(v) => v, Err(_) => return Ok(None) };
        let get = |key: &str| fm.get(key);
        let id = match get("id").and_then(|v| v.as_i64()) { Some(v) => v, None => return Ok(None) };
        let title = match get("title").and_then(|v| v.as_str()) { Some(v) => v.to_string(), None => return Ok(None) };
        let status = get("status").and_then(|v| v.as_str()).unwrap_or("todo");
        let status = match TaskStatus::parse(status) { Ok(s) => s, Err(_) => return Ok(None) };
        let labels = get("labels").and_then(|v| v.as_sequence()).map(|xs| xs.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect()).unwrap_or_default();
        let added_at = get("added_at").and_then(|v| v.as_str())
            .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok())
            .unwrap_or_else(|| Local::now().naive_local());
        Ok(Some(Task { id, title, added_at, status, labels, body: parts[2].trim().to_string() }))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn task(id: i64, title: &str) -> Task {
        Task {
            id, title: title.into(),
            added_at: NaiveDateTime::parse_from_str("2026-05-01T10:00:00", "%Y-%m-%dT%H:%M:%S").unwrap(),
            status: TaskStatus::Todo, labels: vec!["p1".into()], body: "details".into(),
        }
    }

    #[test]
    fn list_format_matches_reference() {
        assert_eq!(task(5, "write docs").format_list(), "(#5) [p1] {⏳} write docs");
    }

    #[test]
    fn frontmatter_round_trip() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        let original = task(1, "sample task");
        store.write_task(&original).unwrap();
        let restored = store.read_task(1).unwrap().unwrap();
        assert_eq!(restored.id, original.id);
        assert_eq!(restored.title, original.title);
        assert_eq!(restored.added_at, original.added_at);
        assert_eq!(restored.labels, original.labels);
        assert_eq!(restored.body, original.body);
    }

    #[test]
    fn index_round_trip_uses_naive_iso_datetime() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        let original = task(1, "sample task");
        store.write_index(&[original.clone()]).unwrap();
        let raw = fs::read_to_string(&store.index).unwrap();
        assert!(raw.contains("2026-05-01T10:00:00"));
        assert!(!raw.contains("+00:00"));
        assert_eq!(store.read_index().unwrap()[0], original);
    }

    #[test]
    fn filter_combines_status_label_and_query() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        let mut a = task(1, "first");
        a.status = TaskStatus::Done;
        a.labels = vec!["p1".into(), "bug".into()];
        let b = task(2, "second");
        store.write_index(&[a, b]).unwrap();
        let found = store.filter(Some(TaskStatus::Done), &["BUG".into()], 0, Some("fir"), false).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, 1);
    }

    #[test]
    fn resolve_hash_prefixed_id() {
        assert_eq!(Store::resolve_id("  #42 ").unwrap(), 42);
    }
}
