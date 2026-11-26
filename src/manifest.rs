use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub const SCHEMA: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MarketplaceIndex {
    pub schema: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub apps: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Requirements {
    pub disk_gb: Option<f64>,
    pub webview2: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct FaqItem {
    pub question: String,
    pub answer: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingKind {
    Text,
    Number,
    Port,
    Boolean,
    Path,
    File,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppSetting {
    pub key: String,
    pub label: String,
    pub kind: SettingKind,
    pub default: Value,
    #[serde(default)]
    pub help: Option<String>,
    /// App settings tab this field belongs to. Without a group the field is listed
    /// on the default tab.
    #[serde(default)]
    pub group: Option<String>,
    /// Environment variables that take the setting out of Autark's hands when the
    /// machine already defines any of them.
    #[serde(default)]
    pub locked_by: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Source {
    pub url: String,
    #[serde(default = "default_branch")]
    pub branch: String,
}

fn default_branch() -> String {
    "main".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Step {
    pub name: String,
    pub run: Vec<String>,
    #[serde(default)]
    pub optional: bool,
    /// Step runs only when this boolean setting is enabled.
    #[serde(default)]
    pub when_setting: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Action {
    pub id: String,
    pub label: String,
    pub run: Vec<String>,
    /// Opened in the browser after the command succeeds.
    #[serde(default)]
    pub url: Option<String>,
    /// Ask the user before running it.
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Setup {
    pub tools: BTreeMap<String, String>,
    pub source: Option<Source>,
    pub installed_check: Option<String>,
    pub version_check: Option<Vec<String>>,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Launch {
    pub run: Vec<String>,
    /// Graceful shutdown command; Autark still kills the process tree afterwards.
    #[serde(default)]
    pub stop: Vec<String>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Manifest {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub homepage: Option<String>,
    pub license: Option<String>,
    pub upstream: Option<String>,
    pub icon: Option<String>,
    pub requirements: Requirements,
    /// Optional questions shown on the app page; an empty list hides the section.
    #[serde(default)]
    pub faq: Vec<FaqItem>,
    pub settings: Vec<AppSetting>,
    pub setup: Setup,
    pub launch: Launch,
    #[serde(default)]
    pub actions: Vec<Action>,
    pub env: BTreeMap<String, String>,
    /// Env names that win over the manifest when the parent already sets them.
    #[serde(default)]
    pub env_keep_parent: Vec<String>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            id: String::new(),
            name: String::new(),
            description: String::new(),
            version: String::new(),
            homepage: None,
            license: None,
            upstream: None,
            icon: None,
            requirements: Requirements::default(),
            faq: Vec::new(),
            settings: Vec::new(),
            setup: Setup::default(),
            launch: Launch::default(),
            actions: Vec::new(),
            env: BTreeMap::new(),
            env_keep_parent: Vec::new(),
        }
    }
}

impl Manifest {
    pub fn load(dir: &Path) -> Result<Self> {
        let folder = dir
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        let path = dir.join("app.json");
        let text = fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        let manifest: Manifest = serde_json::from_str(&text)
            .with_context(|| format!("invalid manifest {}", path.display()))?;
        manifest
            .validate(&folder)
            .with_context(|| format!("invalid manifest {}", path.display()))?;
        Ok(manifest)
    }

    pub fn validate(&self, folder_id: &str) -> Result<()> {
        if self.schema != SCHEMA {
            bail!("invalid_argument: unsupported manifest schema {}", self.schema);
        }
        if self.id != folder_id {
            bail!(
                "invalid_argument: manifest id `{}` does not match folder `{}`",
                self.id,
                folder_id
            );
        }
        if !is_app_id(&self.id) {
            bail!("invalid_argument: app id `{}` is not [a-z0-9-]+", self.id);
        }
        for (field, value) in [
            ("name", &self.name),
            ("description", &self.description),
            ("version", &self.version),
        ] {
            if value.trim().is_empty() {
                bail!("invalid_argument: manifest field `{field}` is empty");
            }
        }
        if !self.launch.run.is_empty() && self.launch.run[0].trim().is_empty() {
            bail!("invalid_argument: `launch.run` must start with a program");
        }
        if !self.launch.stop.is_empty() && self.launch.stop[0].trim().is_empty() {
            bail!("invalid_argument: `launch.stop` must start with a program");
        }
        if let Some(url) = &self.launch.url {
            self.check_template(url, "launch.url")?;
        }
        for (index, step) in self.setup.steps.iter().enumerate() {
            if step.name.trim().is_empty() {
                bail!("invalid_argument: setup step {index} has no name");
            }
            if step.run.is_empty() || step.run[0].trim().is_empty() {
                bail!("invalid_argument: setup step `{}` has no program", step.name);
            }
            if let Some(key) = &step.when_setting {
                let known = self
                    .settings
                    .iter()
                    .any(|s| &s.key == key && s.kind == SettingKind::Boolean);
                if !known {
                    bail!(
                        "invalid_argument: setup step `{}` gates on `{key}`, which is not a boolean setting",
                        step.name
                    );
                }
            }
            for arg in &step.run {
                self.check_template(arg, &step.name)?;
            }
        }
        for arg in &self.launch.run {
            self.check_template(arg, "launch.run")?;
        }
        for arg in &self.launch.stop {
            self.check_template(arg, "launch.stop")?;
        }
        let mut action_ids = std::collections::BTreeSet::new();
        for action in &self.actions {
            if action.id.is_empty() {
                bail!("invalid_argument: action id is empty");
            }
            if !is_app_id(&action.id) {
                bail!("invalid_argument: action id `{}` is not [a-z0-9-]+", action.id);
            }
            if !action_ids.insert(action.id.clone()) {
                bail!("invalid_argument: duplicate action `{}`", action.id);
            }
            if action.label.trim().is_empty() {
                bail!("invalid_argument: action `{}` has no label", action.id);
            }
            if action.run.is_empty() || action.run[0].trim().is_empty() {
                bail!("invalid_argument: action `{}` has no program", action.id);
            }
            for arg in &action.run {
                self.check_template(arg, "action.run")?;
            }
            if let Some(url) = &action.url {
                self.check_template(url, "action.url")?;
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    bail!("invalid_argument: action url must be http or https");
                }
            }
        }
        if let Some(check) = &self.setup.installed_check {
            self.check_template(check, "setup.installed_check")?;
        }
        if let Some(check) = &self.setup.version_check {
            if check.is_empty() || check[0].trim().is_empty() {
                bail!("invalid_argument: `setup.version_check` has no program");
            }
            for arg in check {
                self.check_template(arg, "setup.version_check")?;
            }
        }
        for (key, value) in &self.env {
            if key.trim().is_empty() {
                bail!("invalid_argument: manifest env key is empty");
            }
            self.check_template(value, "env")?;
        }
        for name in &self.env_keep_parent {
            if name.trim().is_empty() {
                bail!("invalid_argument: manifest env_keep_parent entry is empty");
            }
        }
        if let Some(source) = &self.setup.source {
            if !source.url.starts_with("https://") {
                bail!("invalid_argument: source url must be https");
            }
            if source.branch.trim().is_empty() {
                bail!("invalid_argument: source branch is empty");
            }
        }
        let mut keys = std::collections::BTreeSet::new();
        for setting in &self.settings {
            if setting.key.trim().is_empty() {
                bail!("invalid_argument: setting key is empty");
            }
            if !keys.insert(setting.key.clone()) {
                bail!("invalid_argument: duplicate setting `{}`", setting.key);
            }
            validate_value(setting.kind, &setting.key, &setting.default)?;
            for name in &setting.locked_by {
                if name.trim().is_empty() {
                    bail!(
                        "invalid_argument: setting `{}` has an empty locked_by entry",
                        setting.key
                    );
                }
            }
        }
        Ok(())
    }

    fn check_template(&self, template: &str, field: &str) -> Result<()> {
        for name in placeholders(template) {
            if name == "root" || name == "app" || name == "source" || name == "data" {
                continue;
            }
            if let Some(key) = name.strip_prefix("settings.") {
                if self.settings.iter().any(|setting| setting.key == key) {
                    continue;
                }
            }
            bail!(
                "invalid_argument: `{field}` uses unknown placeholder `{{{name}}}`"
            );
        }
        Ok(())
    }

    pub fn defaults(&self) -> Map<String, Value> {
        self.settings
            .iter()
            .map(|setting| (setting.key.clone(), setting.default.clone()))
            .collect()
    }

    pub fn icon_path(&self, manifest_dir: &Path) -> Option<String> {
        let icon = self.icon.as_ref()?;
        let path = manifest_dir.join(icon);
        path.exists().then(|| path.to_string_lossy().to_string())
    }
}

pub fn is_app_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

fn placeholders(template: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let tail = &rest[start + 1..];
        let Some(end) = tail.find('}') else { break };
        let name = &tail[..end];
        if is_placeholder_name(name) {
            names.push(name.to_string());
        }
        rest = &tail[end + 1..];
    }
    names
}

fn is_placeholder_name(name: &str) -> bool {
    !name.is_empty()
        && name.starts_with(|ch: char| ch.is_ascii_lowercase())
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '.')
}

/// Expands `{root}`, `{app}`, `{source}`, `{data}` and `{settings.<key>}` in a manifest template.
pub fn expand(template: &str, root: &Path, app: &Path, values: &Map<String, Value>) -> Result<String> {
    expand_depth(template, root, app, values, 0)
}

/// Setting values may hold their own placeholders, such as a default of `{app}/huggingface`.
fn expand_depth(
    template: &str,
    root: &Path,
    app: &Path,
    values: &Map<String, Value>,
    depth: u8,
) -> Result<String> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let Some(end) = tail.find('}') else {
            out.push('{');
            rest = tail;
            continue;
        };
        let name = &tail[..end];
        if !is_placeholder_name(name) {
            out.push('{');
            out.push_str(&tail[..=end]);
            rest = &tail[end + 1..];
            continue;
        }
        let resolved = match name {
            "root" => root.to_string_lossy().to_string(),
            "app" => app.to_string_lossy().to_string(),
            "source" => app.join("source").to_string_lossy().to_string(),
            "data" => app.join("user-data").to_string_lossy().to_string(),
            _ => {
                let key = name.strip_prefix("settings.").with_context(|| {
                    format!("invalid_argument: unknown placeholder `{{{name}}}`")
                })?;
                let value = values.get(key).with_context(|| {
                    format!("invalid_argument: unknown setting `{key}` in placeholder")
                })?;
                match value {
                    Value::String(text) if depth < 4 => {
                        expand_depth(text, root, app, values, depth + 1)?
                    }
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                }
            }
        };
        out.push_str(&resolved);
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn validate_value(kind: SettingKind, key: &str, value: &Value) -> Result<()> {
    match kind {
        SettingKind::Text | SettingKind::Path | SettingKind::File => {
            if !value.is_string() {
                bail!("invalid_argument: setting `{key}` must be a string");
            }
        }
        SettingKind::Number => {
            if !value.is_number() {
                bail!("invalid_argument: setting `{key}` must be a number");
            }
        }
        SettingKind::Boolean => {
            if !value.is_boolean() {
                bail!("invalid_argument: setting `{key}` must be a boolean");
            }
        }
        SettingKind::Port => {
            let port = value.as_u64().unwrap_or(0);
            if !(1..=65535).contains(&port) {
                bail!("invalid_argument: setting `{key}` must be a port between 1 and 65535");
            }
        }
    }
    Ok(())
}

pub fn app_dir(root: &Path, id: &str) -> PathBuf {
    root.join("apps").join(id)
}

pub fn load_settings(app: &Path, manifest: &Manifest) -> Map<String, Value> {
    let mut values = manifest.defaults();
    let path = app.join("settings.json");
    if let Ok(text) = fs::read_to_string(path) {
        if let Ok(stored) = serde_json::from_str::<Map<String, Value>>(&text) {
            for setting in &manifest.settings {
                if let Some(value) = stored.get(&setting.key) {
                    if validate_value(setting.kind, &setting.key, value).is_ok() {
                        values.insert(setting.key.clone(), value.clone());
                    }
                }
            }
        }
    }
    values
}

pub fn save_settings(app: &Path, values: &Map<String, Value>) -> Result<()> {
    fs::create_dir_all(app)?;
    fs::write(
        app.join("settings.json"),
        serde_json::to_string_pretty(values)?,
    )
    .context("cannot write app settings.json")
}

pub fn validate_settings(manifest: &Manifest, values: &Map<String, Value>, app: &Path) -> Result<Map<String, Value>> {
    let mut clean = manifest.defaults();
    for (key, value) in values {
        let setting = manifest
            .settings
            .iter()
            .find(|setting| &setting.key == key)
            .with_context(|| format!("invalid_argument: unknown setting `{key}`"))?;
        validate_value(setting.kind, &setting.key, value)?;
        let value = match (setting.kind, value) {
            (SettingKind::Path | SettingKind::File, Value::String(path)) => Value::String(portable_path(path, app)),
            _ => value.clone(),
        };
        clean.insert(key.clone(), value);
    }
    Ok(clean)
}

/// Rewrites a path that points inside the app folder to the portable `{app}` form.
pub fn portable_path(value: &str, app: &Path) -> String {
    let to_slash = |text: &str| text.replace('\\', "/");
    let trim = |text: String| text.trim_end_matches('/').to_string();
    let dir = trim(to_slash(&app.to_string_lossy())).to_lowercase();
    let full = trim(to_slash(value));
    let lower = full.to_lowercase();
    if lower == dir {
        return "{app}".to_string();
    }
    if lower.starts_with(&format!("{dir}/")) {
        return format!("{{app}}{}", &full[dir.len()..]);
    }
    full
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstalledRecord {
    pub version: String,
    #[serde(default)]
    pub app_version: Option<String>,
    #[serde(default)]
    pub installed_at: u64,
    /// The preview branch or pull request the app was installed from; empty means stable.
    #[serde(default)]
    pub source: Option<String>,
}

pub fn read_installed(app: &Path) -> Option<InstalledRecord> {
    let text = fs::read_to_string(app.join("installed.json")).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn write_installed(app: &Path, version: &str, app_version: Option<&str>, source: &str) -> Result<()> {
    let installed_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let record = InstalledRecord {
        version: version.to_string(),
        app_version: app_version.map(str::to_string),
        installed_at,
        source: (!source.is_empty()).then(|| source.to_string()),
    };
    fs::create_dir_all(app)?;
    fs::write(
        app.join("installed.json"),
        serde_json::to_string_pretty(&record)?,
    )
    .context("cannot write installed.json")
}

pub fn snapshot_manifest(app: &Path, manifest: &Manifest) -> Result<()> {
    fs::write(
        app.join("app.json"),
        serde_json::to_string_pretty(manifest)?,
    )
    .context("cannot write app.json snapshot")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo(gate: &str, settings: &str) -> String {
        format!(
            r#"{{"schema":1,"id":"demo","name":"Demo","description":"d","version":"1",
            "settings":{settings},
            "setup":{{"steps":[{{"name":"copy","when_setting":"{gate}","run":["powershell","-Command","echo hi"]}}]}},
            "launch":{{"run":["demo.exe"]}}}}"#
        )
    }

    #[test]
    fn step_gate_must_be_a_boolean_setting() {
        let settings = r#"[{"key":"import_config","label":"Copy config","kind":"boolean","default":false}]"#;
        let ok: Manifest = serde_json::from_str(&demo("import_config", settings)).unwrap();
        ok.validate("demo").unwrap();

        let unknown: Manifest = serde_json::from_str(&demo("missing", settings)).unwrap();
        assert!(unknown.validate("demo").is_err());

        let text_kind = r#"[{"key":"import_config","label":"Copy config","kind":"text","default":""}]"#;
        let wrong: Manifest = serde_json::from_str(&demo("import_config", text_kind)).unwrap();
        assert!(wrong.validate("demo").is_err());
    }

    #[test]
    fn env_keep_parent_rejects_empty_names() {
        let json = r#"{"schema":1,"id":"demo","name":"Demo","description":"d","version":"1",
            "launch":{"run":["demo.exe"]},"env_keep_parent":[" "]}"#;
        let manifest: Manifest = serde_json::from_str(json).unwrap();
        assert!(manifest.validate("demo").is_err());
    }

    #[test]
    fn setting_defaults_may_hold_placeholders() {
        let settings = r#"[{"key":"import_config","label":"Copy","kind":"boolean","default":false},
            {"key":"hf_home","label":"Models","kind":"path","default":"{app}/huggingface","locked_by":["HF_HOME"]}]"#;
        let manifest: Manifest = serde_json::from_str(&demo("import_config", settings)).unwrap();
        assert!(manifest.validate("demo").is_ok());
        let values = manifest.defaults();
        let app = Path::new("C:/autark/apps/demo");
        let expanded = expand("{settings.hf_home}/hub", Path::new("C:/autark"), app, &values).unwrap();
        assert_eq!(expanded, "C:/autark/apps/demo/huggingface/hub");
    }

    #[test]
    fn locked_by_rejects_empty_names() {
        let settings = r#"[{"key":"hf_home","label":"Models","kind":"path","default":"x","locked_by":[" "]}]"#;
        let manifest: Manifest = serde_json::from_str(&demo("import_config", settings)).unwrap();
        assert!(manifest.validate("demo").is_err());
    }

    #[test]
    fn picked_paths_inside_the_app_folder_become_portable() {
        let app = Path::new("C:/autark/apps/demo");
        assert_eq!(portable_path("C:\\autark\\apps\\demo\\huggingface", app), "{app}/huggingface");
        assert_eq!(portable_path("c:/AUTARK/apps/demo", app), "{app}");
        assert_eq!(portable_path("D:\\Models", app), "D:/Models");
    }

    #[test]
    fn actions_validate_ids_and_urls() {
        let duplicate = r#"{"schema":1,"id":"demo","name":"Demo","description":"d","version":"1",
            "launch":{"run":["demo.exe"]},"actions":[
            {"id":"refresh","label":"Refresh","run":["demo.exe"]},
            {"id":"refresh","label":"Again","run":["demo.exe"]}]}"#;
        let manifest: Manifest = serde_json::from_str(duplicate).unwrap();
        assert!(manifest.validate("demo").is_err());
        let valid = r#"{"schema":1,"id":"demo","name":"Demo","description":"d","version":"1",
            "launch":{"run":["demo.exe"]},"actions":[
            {"id":"refresh","label":"Refresh","run":["demo.exe"],"url":"https://example.com"}]}"#;
        let manifest: Manifest = serde_json::from_str(valid).unwrap();
        assert!(manifest.validate("demo").is_ok());
    }

    #[test]
    fn launch_is_optional_and_stop_is_checked() {
        let no_launch = r#"{"schema":1,"id":"demo","name":"Demo","description":"d","version":"1"}"#;
        let manifest: Manifest = serde_json::from_str(no_launch).unwrap();
        assert!(manifest.validate("demo").is_ok());
        assert!(manifest.launch.run.is_empty());

        let with_stop = r#"{"schema":1,"id":"demo","name":"Demo","description":"d","version":"1",
            "launch":{"run":["demo.exe"],"stop":["demo.exe","--quit"]}}"#;
        let manifest: Manifest = serde_json::from_str(with_stop).unwrap();
        manifest.validate("demo").unwrap();
        assert_eq!(manifest.launch.stop.len(), 2);

        let blank_stop = r#"{"schema":1,"id":"demo","name":"Demo","description":"d","version":"1",
            "launch":{"run":["demo.exe"],"stop":[" "]}}"#;
        let manifest: Manifest = serde_json::from_str(blank_stop).unwrap();
        assert!(manifest.validate("demo").is_err());
    }
}
