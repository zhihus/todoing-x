use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, Timelike};
use serde::{de, Deserializer, Serializer};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, io::Write, path::{Path, PathBuf}};
use tempfile::NamedTempFile;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskDateTime {
    Naive(NaiveDateTime),
    Offset(DateTime<FixedOffset>),
}

impl TaskDateTime {
    pub fn parse_iso(value: &str) -> Result<Self, String> {
        let normalized = if value.as_bytes().get(10) == Some(&b' ') {
            format!("{}T{}", &value[..10], &value[11..])
        } else {
            value.to_string()
        };
        if let Ok(value) = NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%dT%H:%M:%S%.f") {
            return Ok(Self::Naive(value));
        }
        if let Ok(value) = NaiveDateTime::parse_from_str(&normalized, "%Y-%m-%dT%H:%M:%S") {
            return Ok(Self::Naive(value));
        }
        if let Ok(date) = chrono::NaiveDate::parse_from_str(&normalized, "%Y-%m-%d") {
            if let Some(value) = date.and_hms_opt(0, 0, 0) {
                return Ok(Self::Naive(value));
            }
        }
        DateTime::parse_from_rfc3339(&normalized)
            .map(Self::Offset)
            .map_err(|e| e.to_string())
    }

    pub fn isoformat(&self) -> String {
        match self {
            Self::Naive(value) => {
                let base = value.format("%Y-%m-%dT%H:%M:%S").to_string();
                let micros = value.and_utc().timestamp_subsec_micros();
                if micros == 0 { base } else { format!("{}.{:06}", base, micros) }
            }
            Self::Offset(value) => {
                let base = value.format("%Y-%m-%dT%H:%M:%S").to_string();
                let micros = value.timestamp_subsec_micros();
                let offset = value.format("%:z").to_string();
                if micros == 0 { format!("{}{}", base, offset) }
                else { format!("{}.{:06}{}", base, micros, offset) }
            },
        }
    }
}

impl Serialize for TaskDateTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        serializer.serialize_str(&self.isoformat())
    }
}

impl<'de> Deserialize<'de> for TaskDateTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        let value = String::deserialize(deserializer)?;
        Self::parse_iso(&value).map_err(de::Error::custom)
    }
}

impl From<NaiveDateTime> for TaskDateTime {
    fn from(value: NaiveDateTime) -> Self { Self::Naive(value) }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub id: i64,
    pub title: String,
    #[serde(default = "default_added_at")]
    pub added_at: TaskDateTime,
    #[serde(default)]
    pub status: TaskStatus,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub body: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<String>,
}

fn default_added_at() -> TaskDateTime { TaskDateTime::Naive(Local::now().naive_local()) }

impl Task {
    pub fn new(id: i64, title: String, status: TaskStatus, labels: Vec<String>, body: String) -> Self {
        let mut seen = HashSet::new();
        let labels = labels.into_iter().filter(|s| seen.insert(s.clone())).collect();
        let now = Local::now().naive_local();
        let added_at = now.with_nanosecond((now.nanosecond() / 1_000) * 1_000).unwrap_or(now);
        Self { id, title, added_at: TaskDateTime::Naive(added_at), status, labels, body, commits: Vec::new() }
    }
    pub fn validate_for_creation(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("Task title cannot be empty".into());
        }
        if self.labels.iter().any(|label| label.trim().is_empty()) {
            return Err("Task labels cannot be empty".into());
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.commits.iter().any(|sha| sha.trim().is_empty()) {
            return Err("Git commit reference cannot be empty".into());
        }
        Ok(())
    }

    pub fn format_list(&self) -> String {
        format!("(#{}) [{}] {{{}}} {}", self.id, self.labels.join(","), self.status.icon(), self.title)
    }
    pub fn format_see(&self) -> String {
        // Match PyYAML's sorted frontmatter key order and inline scalar-list representation.
        let dt = self.added_at.isoformat();
        let labels = self.labels.iter().map(|s| yaml_scalar(s)).collect::<Vec<_>>().join(", ");
        let commits = if self.commits.is_empty() { String::new() } else { format!("commits: [{}]\n", self.commits.iter().map(|s| yaml_scalar(s)).collect::<Vec<_>>().join(", ")) };
        let mut out = format!("---\nadded_at: '{}'\n{}id: {}\nlabels: [{}]\n", dt, commits, self.id, labels);
        out.push_str(&format!("status: {}\ntitle: {}\n---\n", self.status.as_str(), yaml_scalar(&self.title)));
        let body = self.body.trim();
        if !body.is_empty() { out.push_str("\n"); out.push_str(body); out.push('\n'); }
        out
    }
}

fn yaml_scalar(s: &str) -> String {
    // PyYAML uses YAML 1.1 implicit scalar resolution (yes/no/on/off,
    // dates, numbers, etc.). Quote these strings so Python keeps them as str.
    let lower = s.to_ascii_lowercase();
    let bytes = s.as_bytes();
    let numeric_prefix = bytes.first().is_some_and(u8::is_ascii_digit)
        || (matches!(bytes.first(), Some(b'+') | Some(b'-'))
            && (bytes.get(1).is_some_and(u8::is_ascii_digit)
                || (bytes.get(1) == Some(&b'.')
                    && bytes.get(2).is_some_and(u8::is_ascii_digit))))
        || (bytes.first() == Some(&b'.')
            && bytes.get(1).is_some_and(u8::is_ascii_digit));
    let yaml11_boolean_or_null = matches!(
        lower.as_str(),
        "y" | "yes" | "n" | "no" | "true" | "false" | "on" | "off" | "null" | "~"
            | ".inf" | "+.inf" | "-.inf" | ".nan"
    );
    if numeric_prefix || yaml11_boolean_or_null {
        return format!("'{}'", s.replace('\'', "''"));
    }
    serde_yaml::to_string(s)
        .map(|yaml| yaml.trim_end().to_string())
        .unwrap_or_else(|_| format!("{:?}", s))
}

fn atomic_write(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| format!("Invalid file path: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut temp = NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    temp.write_all(content).map_err(|e| e.to_string())?;
    temp.as_file().sync_all().map_err(|e| e.to_string())?;
    temp.persist(path).map_err(|e| e.error.to_string())?;
    Ok(())
}


fn parse_decimal_digits(value: &str) -> Option<i64> {
    const ZEROES: &[u32] = &[
        0x0030,0x0660,0x06F0,0x07C0,0x0966,0x09E6,0x0A66,0x0AE6,0x0B66,0x0BE6,
        0x0C66,0x0CE6,0x0D66,0x0DE6,0x0E50,0x0ED0,0x0F20,0x1040,0x1090,0x17E0,
        0x1810,0x1946,0x19D0,0x1A80,0x1A90,0x1B50,0x1BB0,0x1C40,0x1C50,0xA620,
        0xA8D0,0xA900,0xA9D0,0xA9F0,0xAA50,0xABF0,0xFF10,0x104A0,0x10D30,0x11066,
        0x110F0,0x11136,0x111D0,0x112F0,0x11450,0x114D0,0x11650,0x116C0,0x11730,
        0x118E0,0x11950,0x11C50,0x11D50,0x11DA0,0x11F50,0x16A60,0x16AC0,0x16B50,
        0x1D7CE,0x1E140,0x1E2F0,0x1E4F0,0x1E950,0x1FBF0,
    ];
    let mut result = 0i64;
    if value.is_empty() { return None; }
    for c in value.chars() {
        let code = c as u32;
        let digit = ZEROES.iter().flat_map(|zero| if *zero == 0x1D7CE { vec![*zero, *zero + 10, *zero + 20, *zero + 30, *zero + 40] } else { vec![*zero] }).find_map(|zero| {
            let n = code.checked_sub(zero)?;
            (n < 10).then_some(n as i64)
        })?;
        result = result.checked_mul(10)?.checked_add(digit)?;
    }
    Some(result)
}

fn parse_python_integer_string(value: &str) -> Option<i64> {
    let value = value.trim();
    let (negative, digits) = if let Some(rest) = value.strip_prefix('-') {
        (true, rest)
    } else if let Some(rest) = value.strip_prefix('+') {
        (false, rest)
    } else {
        (false, value)
    };
    let mut normalized = String::new();
    let chars: Vec<char> = digits.chars().collect();
    for (index, ch) in chars.iter().enumerate() {
        if *ch == '_' {
            if index == 0 || index + 1 == chars.len()
                || parse_decimal_digits(&chars[index - 1].to_string()).is_none()
                || parse_decimal_digits(&chars[index + 1].to_string()).is_none()
            { return None; }
        } else {
            normalized.push(*ch);
        }
    }
    let number = parse_decimal_digits(&normalized)?;
    if negative { number.checked_neg() } else { Some(number) }
}

fn parse_yaml_python_integer(value: &serde_yaml::Value) -> Option<i64> {
    match value {
        serde_yaml::Value::Number(number) => number.as_i64()
            .or_else(|| number.as_u64().and_then(|n| i64::try_from(n).ok()))
            .or_else(|| number.as_f64().filter(|n| n.is_finite() && *n >= i64::MIN as f64 && *n < -(i64::MIN as f64)).map(|n| n.trunc() as i64)),
        serde_yaml::Value::String(value) => parse_python_integer_string(value),
        serde_yaml::Value::Bool(value) => Some(i64::from(*value)),
        _ => None,
    }
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
                        if let Some(n) = parse_decimal_digits(stem) { max_id = max_id.max(n); }
                    }
                }
            }
        }
        max_id + 1
    }
    pub fn resolve_id(raw: &str) -> Result<i64, String> {
        let value = raw.trim().trim_start_matches('#');
        if value.is_empty() {
            return Err(format!("Invalid task ID: {}", value));
        }
        let id = parse_decimal_digits(value)
            .ok_or_else(|| format!("Invalid task ID: {}", value))?;
        Ok(id)
    }
    fn task_path(&self, id: i64) -> PathBuf { self.tasks.join(format!("{}.md", id)) }
    pub fn read_task(&self, id: i64) -> Result<Option<Task>, String> {
        let p = self.task_path(id);
        if !p.exists() { return Ok(None); }
        self.parse_task_file(&p)
    }
    pub fn write_task(&self, task: &Task) -> Result<(), String> {
        task.validate()?;
        self.ensure()?;
        atomic_write(&self.task_path(task.id), task.format_see().as_bytes())
    }
    pub fn delete_task_file(&self, id: i64) -> Result<bool, String> {
        let p = self.task_path(id);
        if !p.exists() { return Ok(false); }
        fs::remove_file(p).map_err(|e| e.to_string())?; Ok(true)
    }
    pub fn read_index(&self) -> Result<Vec<Task>, String> {
        if !self.index.exists() { return self.rebuild_index(); }
        let raw = fs::read_to_string(&self.index).map_err(|e| e.to_string())?;
        let data: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(_) => return self.rebuild_index(),
        };
        let Some(items) = data.as_array() else {
            return self.rebuild_index();
        };
        if items.iter().any(|item| {
            item.as_object().is_none_or(|object| !object.contains_key("id") || !object.contains_key("title"))
        }) {
            return self.rebuild_index();
        }
        let mut tasks = Vec::with_capacity(items.len());
        for item in items {
            let task: Task = serde_json::from_value(item.clone())
                .map_err(|error| format!("Invalid task in index: {}", error))?;
            task.validate()?;
            tasks.push(task);
        }
        Ok(tasks)
    }
    pub fn write_index(&self, tasks: &[Task]) -> Result<(), String> {
        self.ensure()?;
        for task in tasks { task.validate()?; }
        let raw = serde_json::to_string_pretty(tasks).map_err(|e| e.to_string())?;
        atomic_write(&self.index, format!("{}\n", raw).as_bytes())
    }
    pub fn rebuild_index(&self) -> Result<Vec<Task>, String> {
        self.ensure()?;
        let mut paths: Vec<PathBuf> = fs::read_dir(&self.tasks).map_err(|e| e.to_string())?
            .flatten().map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("md")).collect();
        paths.sort_by_key(|p| p.file_stem().and_then(|s| s.to_str()).and_then(|s| parse_decimal_digits(s)).unwrap_or(0));
        let mut tasks = Vec::new();
        for p in paths {
            match self.parse_task_file(&p)? {
                Some(task) => tasks.push(task),
                None => {}
            }
        }
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
    pub fn filter(&self, status: Option<TaskStatus>, labels: &[String], limit: i64, q: Option<&str>, reverse: bool) -> Result<Vec<Task>, String> {
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
        if limit > 0 { tasks.truncate(limit as usize); }
        Ok(tasks)
    }
    fn parse_task_file(&self, path: &Path) -> Result<Option<Task>, String> {
        let text = match fs::read_to_string(path) {
            Ok(s) => s.replace("\r\n", "\n").replace('\r', "\n"),
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                return Err(format!("Cannot decode task file {}: {}", path.display(), error));
            }
            Err(_) => return Ok(None),
        };
        if !text.starts_with("---") { eprintln!("Warning: Task file {} has no frontmatter, skipping", path.display()); return Ok(None); }
        let parts: Vec<&str> = text.splitn(3, "---").collect();
        if parts.len() < 3 { return Ok(None); }
        let fm: serde_yaml::Value = match serde_yaml::from_str(parts[1]) { Ok(v) => v, Err(_) => return Ok(None) };
        if !fm.is_mapping() {
            return Err(format!("Invalid task frontmatter in {}", path.display()));
        }
        let get = |key: &str| fm.get(key);
        let id = get("id").and_then(parse_yaml_python_integer)
            .ok_or_else(|| format!("Invalid or missing task ID in {}", path.display()))?;
        let title = get("title").and_then(|v| v.as_str()).map(str::to_owned)
            .ok_or_else(|| format!("Invalid or missing task title in {}", path.display()))?;
        let status = match get("status") {
            None => "todo",
            Some(value) => value.as_str().ok_or_else(|| format!("Invalid task status in {}", path.display()))?,
        };
        let status = TaskStatus::parse(status)
            .map_err(|_| format!("Invalid task status in {}", path.display()))?;
        let labels = match get("labels") {
            None => Vec::new(),
            Some(value) if value.is_null() => Vec::new(),
            Some(value) => match value.as_sequence() {
                Some(items) => {
                    let mut parsed = Vec::new();
                    for item in items {
                        if let Some(label) = item.as_str() { parsed.push(label.to_owned()); }
                        else { return Err(format!("Invalid task label in {}", path.display())); }
                    }
                    parsed
                }
                None => return Err(format!("Invalid task labels in {}", path.display())),
            },
        };
        let commits = match get("commits") {
            None => Vec::new(),
            Some(value) if value.is_null() => Vec::new(),
            Some(value) => match value.as_sequence() {
                Some(items) => {
                    let mut parsed = Vec::new();
                    for item in items {
                        if let Some(sha) = item.as_str() { parsed.push(sha.to_owned()); }
                        else { return Ok(None); }
                    }
                    parsed
                }
                None => return Err(format!("Invalid task commits in {}", path.display())),
            },
        };
        let added_at = match get("added_at") {
            None => TaskDateTime::Naive(Local::now().naive_local()),
            Some(value) => match value.as_str().and_then(|s| TaskDateTime::parse_iso(s).ok()) {
                Some(value) => value,
                None => return Err(format!("Invalid task added_at in {}", path.display())),
            },
        };
        Ok(Some(Task { id, title, added_at, status, labels, body: parts[2].trim().to_string(), commits }))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn task(id: i64, title: &str) -> Task {
        Task {
            id, title: title.into(), commits: Vec::new(),
            added_at: TaskDateTime::Naive(NaiveDateTime::parse_from_str("2026-05-01T10:00:00", "%Y-%m-%dT%H:%M:%S").unwrap()),
            status: TaskStatus::Todo, labels: vec!["p1".into()], body: "details".into(),
        }
    }


    #[test]
    fn unicode_decimal_ids_match_python() {
        assert_eq!(Store::resolve_id("١٢").unwrap(), 12);
        assert_eq!(Store::resolve_id("１２").unwrap(), 12);
        assert!(Store::resolve_id("²").is_err());
    }

    #[test]
    fn next_id_accepts_unicode_decimal_filename_stems() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        fs::write(store.tasks.join("١٢.md"), "---\\nid: 12\\ntitle: twelve\\n---\\n").unwrap();
        assert_eq!(store.next_id(), 13);
    }

    #[test]
    fn yaml_id_conversion_matches_python_int() {
        assert_eq!(parse_yaml_python_integer(&serde_yaml::from_str::<serde_yaml::Value>("'+12'").unwrap()), Some(12));
        assert_eq!(parse_yaml_python_integer(&serde_yaml::from_str::<serde_yaml::Value>("'١٢'").unwrap()), Some(12));
        assert_eq!(parse_yaml_python_integer(&serde_yaml::from_str::<serde_yaml::Value>("12.9").unwrap()), Some(12));
        assert_eq!(parse_yaml_python_integer(&serde_yaml::from_str::<serde_yaml::Value>("true").unwrap()), Some(1));
    }


    #[test]
    fn yaml_integer_strings_accept_python_underscore_syntax() {
        assert_eq!(parse_python_integer_string("1_000"), Some(1000));
        assert_eq!(parse_python_integer_string("-١_٢"), Some(-12));
        assert_eq!(parse_python_integer_string("_12"), None);
        assert_eq!(parse_python_integer_string("12_"), None);
    }

    #[test]
    fn rebuild_propagates_invalid_required_frontmatter_fields() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        fs::write(store.task_path(1), "---\nid: 1\nstatus: todo\n---\n").unwrap();
        assert!(store.rebuild_index().is_err());
    }

    #[test]
    fn null_status_is_not_treated_as_missing_status() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        fs::write(store.task_path(1), "---\nid: 1\ntitle: task\nstatus: null\n---\n").unwrap();
        assert!(store.read_task(1).is_err());
    }

    #[test]
    fn index_wrong_top_level_shape_rebuilds_like_python() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.write_task(&task(4, "from markdown")).unwrap();
        fs::write(&store.index, r#"{"not":"an array"}"#).unwrap();
        assert_eq!(store.read_index().unwrap()[0].title, "from markdown");
    }

    #[test]
    fn index_missing_required_key_rebuilds_like_python() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.write_task(&task(4, "from markdown")).unwrap();
        fs::write(&store.index, r#"[{"id":4}]"#).unwrap();
        assert_eq!(store.read_index().unwrap()[0].title, "from markdown");
    }

    #[test]
    fn index_task_validation_errors_are_not_silently_rebuilt() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.write_task(&task(4, "from markdown")).unwrap();
        fs::write(&store.index, r#"[{"id":4,"title":"bad","status":"blocked"}]"#).unwrap();
        assert!(store.read_index().is_err());
    }

    #[test]
    fn list_format_matches_reference() {
        assert_eq!(task(5, "write docs").format_list(), "(#5) [p1] {⏳} write docs");
    }

    #[test]
    fn yaml_scalar_preserves_python_yaml_11_string_types() {
        for value in ["123", "01", "yes", "NO", "on", "off", "null", "~", "2026-05-01", "1e3", ".5", "+.5", "-.5"] {
            let serialized = yaml_scalar(value);
            assert!(serialized.starts_with('\''), "{value:?} was not quoted: {serialized}");
        }

        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        for (id, value) in ["yes", "on", "2026-05-01"].into_iter().enumerate() {
            let original = task(id as i64 + 12, value);
            store.write_task(&original).unwrap();
            let restored = store.read_task(original.id).unwrap().unwrap();
            assert_eq!(restored.title, value);
        }
    }

    #[test]
    fn reads_unquoted_python_yaml_timestamp() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        let fixture = "---\nadded_at: 2026-05-01T17:15:52.337350\nid: 2\nlabels: []\nstatus: todo\ntitle: timestamp\n---\n";
        fs::write(store.task_path(2), fixture).unwrap();

        let task = store.read_task(2).unwrap().unwrap();
        assert_eq!(task.added_at.isoformat(), "2026-05-01T17:15:52.337350");
    }

    #[test]
    fn reads_task_file_written_by_original_python_version() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        let fixture = "---\nadded_at: '2026-05-01T17:15:52.337350'\nid: 2\nlabels: [enhancement]\nstatus: done\ntitle: 'skill: package for distribution'\n---\n\nCreate a proper packaging story.\n";
        fs::write(store.task_path(2), fixture).unwrap();

        let task = store.read_task(2).unwrap().unwrap();
        assert_eq!(task.id, 2);
        assert_eq!(task.title, "skill: package for distribution");
        assert_eq!(task.status, TaskStatus::Done);
        assert_eq!(task.labels, vec!["enhancement"]);
        assert_eq!(task.body, "Create a proper packaging story.");
        assert_eq!(
            task.added_at,
            TaskDateTime::Naive(NaiveDateTime::parse_from_str("2026-05-01T17:15:52.337350", "%Y-%m-%dT%H:%M:%S%.f").unwrap())
        );
    }

    #[test]
    fn timezone_aware_added_at_round_trips_without_losing_offset() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        let fixture = "---\nadded_at: '2026-05-01T17:15:52.337350+03:00'\nid: 7\nlabels: []\nstatus: todo\ntitle: zoned task\n---\n";
        fs::write(store.task_path(7), fixture).unwrap();
        let task = store.read_task(7).unwrap().unwrap();
        assert_eq!(task.added_at.isoformat(), "2026-05-01T17:15:52.337350+03:00");
        store.write_index(&[task.clone()]).unwrap();
        let raw = fs::read_to_string(&store.index).unwrap();
        assert!(raw.contains("2026-05-01T17:15:52.337350+03:00"));
        assert_eq!(store.read_index().unwrap()[0], task);
        store.write_task(&task).unwrap();
        let markdown = fs::read_to_string(store.task_path(7)).unwrap();
        assert!(markdown.contains("2026-05-01T17:15:52.337350+03:00"));
    }

    #[test]
    fn timezone_aware_added_at_uses_python_microsecond_precision() {
        let short = TaskDateTime::parse_iso("2026-05-01T17:15:52.1+03:00").unwrap();
        assert_eq!(short.isoformat(), "2026-05-01T17:15:52.100000+03:00");

        let precise = TaskDateTime::parse_iso("2026-05-01T17:15:52.123456789+03:00").unwrap();
        assert_eq!(precise.isoformat(), "2026-05-01T17:15:52.123456+03:00");

        let whole = TaskDateTime::parse_iso("2026-05-01T17:15:52+00:00").unwrap();
        assert_eq!(whole.isoformat(), "2026-05-01T17:15:52+00:00");
    }

    #[test]
    fn yaml_date_only_added_at_matches_python_midnight_datetime() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        fs::write(
            store.task_path(1),
            "---\\nadded_at: 2026-05-01\\nid: 1\\ntitle: date-only\\nstatus: todo\\nlabels: []\\n---\\n",
        ).unwrap();

        let task = store.read_task(1).unwrap().unwrap();
        assert_eq!(task.added_at.isoformat(), "2026-05-01T00:00:00");
    }

    #[test]
    fn naive_added_at_remains_without_timezone_suffix() {
        let value = TaskDateTime::parse_iso("2026-05-01T17:15:52.337350").unwrap();
        assert_eq!(value.isoformat(), "2026-05-01T17:15:52.337350");
    }

    #[test]
    fn python_space_separated_iso_datetime_is_accepted() {
        let naive = TaskDateTime::parse_iso("2026-05-01 17:15:52.337350").unwrap();
        assert_eq!(naive.isoformat(), "2026-05-01T17:15:52.337350");

        let offset = TaskDateTime::parse_iso("2026-05-01 17:15:52+03:00").unwrap();
        assert_eq!(offset.isoformat(), "2026-05-01T17:15:52+03:00");
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
    fn negative_limit_matches_original_unlimited_behavior() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.write_index(&[task(1, "one"), task(2, "two"), task(3, "three")]).unwrap();
        let found = store.filter(None, &[], -1, None, false).unwrap();
        assert_eq!(found.iter().map(|t| t.id).collect::<Vec<_>>(), vec![3, 2, 1]);
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
    fn index_deserialization_uses_original_model_defaults() {
        let raw = r#"{"id": 1, "title": "legacy"}"#;
        let parsed: Task = serde_json::from_str(raw).unwrap();
        assert_eq!(parsed.status, TaskStatus::Todo);
        assert!(parsed.labels.is_empty());
        assert!(parsed.body.is_empty());
    }

    #[test]
    fn corrupted_index_is_rebuilt_from_task_files() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.write_task(&task(1, "recovered")).unwrap();
        fs::write(&store.index, "not valid json {{{").unwrap();
        let items = store.read_index().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "recovered");
    }

    #[test]
    fn windows_line_endings_are_normalized_when_reading_task_body() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        fs::write(
            store.task_path(1),
            "---\r\nid: 1\r\ntitle: windows\r\nstatus: todo\r\nlabels: []\r\n---\r\nfirst line\r\nsecond line\r\n",
        ).unwrap();
        let task = store.read_task(1).unwrap().unwrap();
        assert_eq!(task.body, "first line\nsecond line");
    }

    #[test]
    fn invalid_utf8_task_file_propagates_decode_error() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        fs::write(store.task_path(1), [0xff, 0xfe]).unwrap();

        assert!(store.read_task(1).unwrap_err().contains("Cannot decode task file"));
    }

    #[test]
    fn malformed_task_files_are_skipped() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        fs::write(store.task_path(1), "---\nid: 1\ntitle: [broken\n---\nbody").unwrap();
        assert!(store.read_task(1).unwrap().is_none());
    }

    #[test]
    fn missing_index_is_rebuilt() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.write_task(&task(3, "rebuild")).unwrap();
        let items = store.read_index().unwrap();
        assert_eq!(items.len(), 1);
        assert!(store.index.exists());
    }

    #[test]
    fn commit_references_round_trip() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        let mut original = task(8, "linked commit");
        original.commits.push("0123456789abcdef".into());
        store.write_task(&original).unwrap();
        let restored = store.read_task(8).unwrap().unwrap();
        assert_eq!(restored.commits, original.commits);
        store.write_index(&[original.clone()]).unwrap();
        assert_eq!(store.read_index().unwrap()[0].commits, original.commits);
    }

    #[test]
    fn invalid_identifiers_are_rejected() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.write_task(&task(0, "zero id")).unwrap();
        assert_eq!(store.read_task(0).unwrap().unwrap().id, 0);
        assert!(Store::resolve_id("-1").is_err());
        assert_eq!(Store::resolve_id("0").unwrap(), 0);
    }

    #[test]
    fn resolve_hash_prefixed_id() {
        assert_eq!(Store::resolve_id("  #42 ").unwrap(), 42);
    }
    #[test]
    fn next_id_uses_maximum_existing_numeric_filename() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        assert_eq!(store.next_id(), 1);
        store.write_task(&task(1, "one")).unwrap();
        store.write_task(&task(3, "three")).unwrap();
        assert_eq!(store.next_id(), 4);
    }

    #[test]
    fn missing_and_malformed_task_files_are_ignored() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        assert!(store.read_task(99).unwrap().is_none());
        fs::write(store.task_path(98), "plain text").unwrap();
        fs::write(store.task_path(97), "---\\nkey: [broken\\n---\\nbody").unwrap();
        assert!(store.read_task(98).unwrap().is_none());
        assert!(store.read_task(97).unwrap().is_none());
    }

    #[test]
    fn delete_task_file_reports_existing_and_missing_files() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.write_task(&task(4, "delete")).unwrap();
        assert!(store.delete_task_file(4).unwrap());
        assert!(!store.delete_task_file(4).unwrap());
    }

    #[test]
    fn corrupt_and_missing_indexes_rebuild_from_markdown() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.write_task(&task(2, "recover")).unwrap();
        assert_eq!(store.read_index().unwrap()[0].title, "recover");
        fs::write(&store.index, "{ invalid json").unwrap();
        assert_eq!(store.read_index().unwrap()[0].title, "recover");
    }

    #[test]
    fn rebuild_skips_malformed_task_files_and_sorts_by_id() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.write_task(&task(3, "third")).unwrap();
        store.write_task(&task(1, "first")).unwrap();
        store.ensure().unwrap();
        fs::write(store.task_path(2), "not a task").unwrap();
        let rebuilt = store.rebuild_index().unwrap();
        assert_eq!(rebuilt.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 3]);
    }

    #[test]
    fn sync_mutation_inserts_and_updates_without_duplicates() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        let first = task(1, "first");
        store.sync_mutation(&first).unwrap();
        let mut updated = first.clone();
        updated.title = "updated".into();
        store.sync_mutation(&updated).unwrap();
        let index = store.read_index().unwrap();
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].title, "updated");
    }

    #[test]
    fn sync_delete_removes_existing_and_ignores_missing_ids() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.write_index(&[task(1, "one"), task(2, "two")]).unwrap();
        store.sync_delete(1).unwrap();
        store.sync_delete(999).unwrap();
        let index = store.read_index().unwrap();
        assert_eq!(index.iter().map(|t| t.id).collect::<Vec<_>>(), vec![2]);
    }

    #[test]
    fn reads_and_rewrites_real_python_task_with_body() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        let fixture = "---\nadded_at: '2026-05-01T17:15:52.337350'\nid: 2\nlabels: [enhancement]\nstatus: done\ntitle: 'skill: package for distribution'\n---\n\nCreate a proper packaging story — pypi, uv tool install, or similar.\n";
        fs::write(store.task_path(2), fixture).unwrap();

        let original = store.read_task(2).unwrap().unwrap();
        assert_eq!(original.title, "skill: package for distribution");
        assert_eq!(original.body, "Create a proper packaging story — pypi, uv tool install, or similar.");
        assert_eq!(original.labels, vec!["enhancement"]);

        store.write_task(&original).unwrap();
        let rewritten = store.read_task(2).unwrap().unwrap();
        assert_eq!(rewritten, original);

        let mut changed = rewritten;
        changed.status = TaskStatus::InProgress;
        changed.body.push_str("\n\nImplementation verified.");
        store.write_task(&changed).unwrap();
        assert_eq!(store.read_task(2).unwrap().unwrap(), changed);
    }

    #[test]
    fn reads_and_rewrites_real_python_task_with_empty_body_and_labels() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        let fixture = "---\nadded_at: '2026-05-01T17:15:52.478460'\nid: 3\nlabels: [enhancement]\nstatus: todo\ntitle: 'skill: shell completion'\n---\n";
        fs::write(store.task_path(3), fixture).unwrap();

        let original = store.read_task(3).unwrap().unwrap();
        assert_eq!(original.body, "");
        assert_eq!(original.status, TaskStatus::Todo);
        store.write_task(&original).unwrap();
        assert_eq!(store.read_task(3).unwrap().unwrap(), original);
    }

    #[test]
    fn reads_real_python_task_with_plain_yaml_title_and_multiline_body() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        let fixture = "---\nadded_at: '2026-05-01T17:59:21.359817'\nid: 5\nlabels: []\nstatus: done\ntitle: implement version subcommand\n---\n\nAdd a version subcommand that prints the current package version from pyproject.toml.\n";
        fs::write(store.task_path(5), fixture).unwrap();

        let original = store.read_task(5).unwrap().unwrap();
        assert_eq!(original.title, "implement version subcommand");
        assert_eq!(original.labels, Vec::<String>::new());
        assert!(original.body.starts_with("Add a version subcommand"));
        store.write_task(&original).unwrap();
        assert_eq!(store.read_task(5).unwrap().unwrap(), original);
    }

    #[test]
    fn reads_and_rewrites_additional_exact_python_task_fixtures() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();

        let fixtures = [
            (4, "---\nadded_at: '2026-05-01T17:15:52.618668'\nid: 4\nlabels: [docs]\nstatus: done\ntitle: 'docs: write README with usage examples'\n---\n"),
            (5, "---\nadded_at: '2026-05-01T17:59:21.359817'\nid: 5\nlabels: []\nstatus: done\ntitle: implement version subcommand\n---\n\nAdd a version subcommand that prints the current package version from pyproject.toml. Read version via importlib.metadata.version(\"todoing\"). Output format: \"todoing v1.0.2\". Handle case where package metadata is unavailable (fallback to \"unknown\").\n\nImplementation notes:\\n- Added __version__ to src/todoing/__init__.py using importlib.metadata.version()\\n- Added version command to CLI in src/todoing/main.py\\n- Added TestVersion test class in tests/test_cli.py\\n- Verified: runs via both 'todoing version' and 'uv run todoing version'\n"),
            (6, "---\nadded_at: '2026-05-01T18:02:50.275015'\nid: 6\nlabels: []\nstatus: done\ntitle: improve skill and add a new skill called todoing-install (a guide to install\n  todoing)\n---\n"),
            (7, "---\nadded_at: '2026-05-01T18:02:51.364129'\nid: 7\nlabels: []\nstatus: todo\ntitle: better rendering — maybe drop brackets like {} and [] around status and labels\n---\n"),
            (9, "---\nadded_at: '2026-05-02T23:22:26.868559'\nid: 9\nlabels: [enhancement]\nstatus: in_progress\ntitle: Improve ls, remove search\n---\n\n## Plan\n\n### 1. `main.py` — add flags to `ls`, drop `search`\n- Add `-q / --query` param to `ls()` — pass it to `store.filter(q=)`\n- Add `-r / --reverse` param — flip sort to `reverse=not reverse`\n- Remove the `search` command entirely (function + decorator)\n- Merge the `--scan` path into the filtered flow so query/reverse apply consistently\n\n### 2. `task.py` — update `Store.filter()`\n- Add `reverse: bool = False` parameter\n- Change `tasks.sort(key=lambda t: t.id, reverse=True)` to `tasks.sort(key=lambda t: t.id, reverse=not reverse)`\n\n### 3. Tests\n- Remove search-specific tests\n- Add tests for:\n  - `ls -q` filtering\n  - `ls -r` sort order\n  - `ls -q` combined with `-s`, `-l`, `-r`\n\n### 4. `SKILL.md`\n- Replace `search` examples with `ls -q` equivalents\n- Remove `search` from command reference table\n")
        ];

        for (id, fixture) in fixtures {
            fs::write(store.task_path(id), fixture).unwrap();
            let original = store.read_task(id).unwrap().unwrap();
            store.write_task(&original).unwrap();
            let reread = store.read_task(id).unwrap().unwrap();
            assert_eq!(reread, original, "round-trip failed for Python task {id}");
        }

        let folded = store.read_task(6).unwrap().unwrap();
        assert_eq!(folded.title, "improve skill and add a new skill called todoing-install (a guide to install todoing)");
        let literal_newlines = store.read_task(5).unwrap().unwrap();
        assert!(literal_newlines.body.contains("\\n- Added __version__"));
        let markdown_body = store.read_task(9).unwrap().unwrap();
        assert!(markdown_body.body.starts_with("## Plan"));
        assert!(markdown_body.body.contains("### 4. `SKILL.md`"));
    }

    #[test]
    fn reads_and_rewrites_remaining_real_python_task_fixture() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        let fixture = "---\nadded_at: '2026-05-01T18:02:52.413510'\nid: 8\nlabels: []\nstatus: todo\ntitle: support table rendering for todoing ls --table\n---\n";
        fs::write(store.task_path(8), fixture).unwrap();

        let original = store.read_task(8).unwrap().unwrap();
        assert_eq!(original.title, "support table rendering for todoing ls --table");
        assert_eq!(original.body, "");
        store.write_task(&original).unwrap();
        assert_eq!(store.read_task(8).unwrap().unwrap(), original);
    }

    #[test]
    fn task_body_can_contain_frontmatter_delimiter() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        store.ensure().unwrap();
        let fixture = "---\nadded_at: '2026-05-02T12:00:00.000000'\nid: 10\ntitle: delimiter\nstatus: todo\nlabels: []\n---\nBefore\n---\nAfter\n";
        fs::write(store.task_path(10), fixture).unwrap();

        let original = store.read_task(10).unwrap().unwrap();
        assert_eq!(original.body, "Before\n---\nAfter");
        store.write_task(&original).unwrap();
        assert_eq!(store.read_task(10).unwrap().unwrap(), original);
    }

    #[test]
    fn filter_matches_status_value_case_insensitively() {
        let dir = tempdir().unwrap();
        let store = Store::new(dir.path());
        let mut item = task(1, "task");
        item.status = TaskStatus::InProgress;
        store.write_index(&[item]).unwrap();
        let found = store.filter(None, &[], 0, Some("IN_PROGRESS"), false).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, 1);
    }

}
