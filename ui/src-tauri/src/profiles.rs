use std::fs;
use std::path::{Path, PathBuf};

use agg_core::WgConfig;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub endpoint: Option<String>,
    pub address: Option<String>,
    pub mtu: Option<u16>,
    pub obfuscated: bool,
    pub source: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    profiles: Vec<Profile>,
    active_id: Option<String>,
}

#[derive(Clone)]
pub struct Library {
    root: PathBuf,
}

impl Library {
    pub fn open(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(root.join("confs")).map_err(io)?;
        Ok(Self { root })
    }

    pub fn list(&self) -> Result<Vec<Profile>, String> {
        Ok(self.load()?.profiles)
    }

    pub fn active_id(&self) -> Result<Option<String>, String> {
        Ok(self.load()?.active_id)
    }

    pub fn set_active(&self, id: Option<String>) -> Result<(), String> {
        let mut idx = self.load()?;
        if let Some(ref i) = id {
            if !idx.profiles.iter().any(|p| &p.id == i) {
                return Err("unknown profile".into());
            }
        }
        idx.active_id = id;
        self.save(&idx)
    }

    pub fn conf_path(&self, id: &str) -> PathBuf {
        self.root.join("confs").join(format!("{id}.conf"))
    }

    pub fn import_text(&self, name: Option<String>, body: &str, source: &str) -> Result<Profile, String> {
        let cfg = WgConfig::from_str(body).map_err(|e| e.to_string())?;
        let endpoint = cfg.peer().ok().and_then(|p| p.endpoint).map(|e| e.to_string());
        let address = cfg.interface.addresses.first().map(|a| a.to_string());
        let guessed = name
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                endpoint
                    .as_ref()
                    .map(|e| e.split(':').next().unwrap_or(e).to_string())
            })
            .unwrap_or_else(|| "unnamed".into());
        let id = Uuid::new_v4().to_string();
        fs::write(self.conf_path(&id), body).map_err(io)?;
        let profile = Profile {
            id: id.clone(),
            name: unique_name(&self.load()?.profiles, &guessed),
            endpoint,
            address,
            mtu: cfg.interface.mtu,
            obfuscated: cfg.interface.obfuscation_present(),
            source: source.to_string(),
        };
        let mut idx = self.load()?;
        idx.profiles.push(profile.clone());
        if idx.active_id.is_none() {
            idx.active_id = Some(id);
        }
        self.save(&idx)?;
        Ok(profile)
    }

    pub fn import_file(&self, path: &Path) -> Result<Profile, String> {
        let body = fs::read_to_string(path).map_err(io)?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string());
        self.import_text(stem, &body, &path.display().to_string())
    }

    pub fn rename(&self, id: &str, name: &str) -> Result<Profile, String> {
        let mut idx = self.load()?;
        let p = idx
            .profiles
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| "unknown profile".to_string())?;
        p.name = name.trim().to_string();
        if p.name.is_empty() {
            return Err("name cannot be empty".into());
        }
        let out = p.clone();
        self.save(&idx)?;
        Ok(out)
    }

    pub fn remove(&self, id: &str) -> Result<(), String> {
        let mut idx = self.load()?;
        idx.profiles.retain(|p| p.id != id);
        if idx.active_id.as_deref() == Some(id) {
            idx.active_id = idx.profiles.first().map(|p| p.id.clone());
        }
        let _ = fs::remove_file(self.conf_path(id));
        self.save(&idx)
    }

    fn load(&self) -> Result<Index, String> {
        let path = self.root.join("index.json");
        if !path.exists() {
            return Ok(Index::default());
        }
        let raw = fs::read_to_string(path).map_err(io)?;
        serde_json::from_str(&raw).map_err(|e| e.to_string())
    }

    fn save(&self, idx: &Index) -> Result<(), String> {
        let path = self.root.join("index.json");
        let raw = serde_json::to_string_pretty(idx).map_err(|e| e.to_string())?;
        fs::write(path, raw).map_err(io)
    }
}

fn unique_name(existing: &[Profile], wanted: &str) -> String {
    let base = wanted.trim();
    if !existing.iter().any(|p| p.name == base) {
        return base.to_string();
    }
    for n in 2..1000 {
        let candidate = format!("{base} ({n})");
        if !existing.iter().any(|p| p.name == candidate) {
            return candidate;
        }
    }
    format!("{base}-{}", Uuid::new_v4().simple())
}

fn io(e: std::io::Error) -> String {
    e.to_string()
}
