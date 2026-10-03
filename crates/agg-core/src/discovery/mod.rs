//! Read-only game / launcher / voice-app scanner. No writes to game folders.

mod vdf;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppSource {
    Steam,
    Epic,
    Riot,
    BattleNet,
    Gog,
    Discord,
    Manual,
}

impl AppSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Steam => "Steam",
            Self::Epic => "Epic",
            Self::Riot => "Riot",
            Self::BattleNet => "Battle.net",
            Self::Gog => "GOG",
            Self::Discord => "Discord",
            Self::Manual => "Manual",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DetectedApp {
    pub id: String,
    pub name: String,
    pub source: AppSource,
    pub executable: PathBuf,
    pub install_dir: PathBuf,
    #[serde(default)]
    pub icon_path: Option<PathBuf>,
    #[serde(default)]
    pub missing: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ScanRoots {
    pub steam_libraries: Vec<PathBuf>,
    pub epic_launcher_dat: Option<PathBuf>,
    pub riot_roots: Vec<PathBuf>,
    pub discord_roots: Vec<PathBuf>,
    pub battlenet_agent: Option<PathBuf>,
    pub gog_config: Option<PathBuf>,
}

pub fn windows_roots() -> ScanRoots {
    let mut r = ScanRoots::default();
    let pf86 = env_dir("ProgramFiles(x86)");
    let pf = env_dir("ProgramFiles");
    let pd = env_dir("ProgramData");
    let local = env_dir("LOCALAPPDATA");

    if let Some(root) = pf86.as_ref() {
        let steam_vdf = root.join("Steam").join("steamapps").join("libraryfolders.vdf");
        r.steam_libraries = steam_libraries_from_vdf(&steam_vdf);
        if r.steam_libraries.is_empty() {
            let fallback = root.join("Steam");
            if fallback.exists() {
                r.steam_libraries.push(fallback);
            }
        }
    }

    if let Some(pd) = pd.as_ref() {
        let epic = pd
            .join("Epic")
            .join("UnrealEngineLauncher")
            .join("LauncherInstalled.dat");
        if epic.exists() {
            r.epic_launcher_dat = Some(epic);
        }
        let bnet = pd.join("Battle.net").join("Agent");
        if bnet.exists() {
            r.battlenet_agent = Some(bnet);
        }
        let gog = pd
            .join("GOG.com")
            .join("Galaxy")
            .join("config")
            .join("config.json");
        if gog.exists() {
            r.gog_config = Some(gog);
        }
    }

    for base in [local.as_ref(), pf.as_ref(), pf86.as_ref()].into_iter().flatten() {
        let riot = base.join("Riot Games");
        if riot.exists() {
            r.riot_roots.push(riot);
        }
    }

    if let Some(local) = local.as_ref() {
        for name in ["Discord", "DiscordPTB", "DiscordCanary"] {
            let p = local.join(name);
            if p.exists() {
                r.discord_roots.push(p);
            }
        }
    }
    r
}

fn env_dir(key: &str) -> Option<PathBuf> {
    std::env::var_os(key).map(PathBuf::from)
}

pub fn scan(roots: &ScanRoots) -> Vec<DetectedApp> {
    let mut out = Vec::new();
    for lib in &roots.steam_libraries {
        steam_library(lib, &mut out);
    }
    if let Some(p) = &roots.epic_launcher_dat {
        epic(p, &mut out);
    }
    for root in &roots.riot_roots {
        riot(root, &mut out);
    }
    for root in &roots.discord_roots {
        discord(root, &mut out);
    }
    if let Some(p) = &roots.battlenet_agent {
        battlenet(p, &mut out);
    }
    if let Some(p) = &roots.gog_config {
        gog(p, &mut out);
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

pub fn mark_missing(apps: &mut [DetectedApp]) {
    for a in apps {
        a.missing = !a.executable.exists();
    }
}

fn steam_libraries_from_vdf(path: &Path) -> Vec<PathBuf> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(v) = vdf::parse(&raw) else {
        tracing::info!("skip steam libraryfolders.vdf (parse)");
        return Vec::new();
    };
    let Some(folders) = v.get("libraryfolders").and_then(|x| x.as_map()) else {
        return Vec::new();
    };
    folders
        .values()
        .filter_map(|e| e.get("path").and_then(|p| p.as_str()).map(PathBuf::from))
        .collect()
}

fn skip_steam_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("soundtrack")
        || n.contains("sdk")
        || n.contains("dedicated server")
        || n.contains("proton")
        || n.contains("steamworks")
        || n.contains("server") && n.contains("tool")
}

fn steam_library(lib: &Path, out: &mut Vec<DetectedApp>) {
    let apps = lib.join("steamapps");
    let Ok(rd) = std::fs::read_dir(&apps) else {
        tracing::info!("skip steam library {} (missing steamapps)", lib.display());
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&p) else {
            continue;
        };
        let Ok(v) = vdf::parse(&raw) else {
            tracing::info!("skip {name} (parse)");
            continue;
        };
        let st = v.get("AppState").unwrap_or(&v);
        let Some(appid) = st.get("appid").and_then(|x| x.as_str()) else {
            continue;
        };
        let Some(title) = st.get("name").and_then(|x| x.as_str()) else {
            continue;
        };
        if skip_steam_name(title) {
            tracing::info!("skip steam {appid} {title}");
            continue;
        }
        let installdir = st
            .get("installdir")
            .and_then(|x| x.as_str())
            .unwrap_or(title);
        let dir = apps.join("common").join(installdir);
        let Some(exe) = find_exe(&dir) else {
            tracing::info!("skip steam {appid} {title} (no exe in {})", dir.display());
            continue;
        };
        tracing::info!("found steam:{appid} {title}");
        out.push(DetectedApp {
            id: format!("steam:{appid}"),
            name: title.to_string(),
            source: AppSource::Steam,
            executable: exe,
            install_dir: dir,
            icon_path: None,
            missing: false,
        });
    }
}

fn epic(path: &Path, out: &mut Vec<DetectedApp>) {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        tracing::info!("skip Epic LauncherInstalled.dat (parse)");
        return;
    };
    let Some(list) = v.get("InstallationList").and_then(|x| x.as_array()) else {
        return;
    };
    for item in list {
        let app = item
            .get("AppName")
            .and_then(|x| x.as_str())
            .unwrap_or("epic");
        let loc = item
            .get("InstallLocation")
            .and_then(|x| x.as_str())
            .map(PathBuf::from);
        let Some(dir) = loc else { continue };
        let Some(exe) = find_exe(&dir) else {
            tracing::info!("skip epic:{app} (no exe)");
            continue;
        };
        let name = item
            .get("AppName")
            .and_then(|x| x.as_str())
            .unwrap_or("Epic game")
            .to_string();
        tracing::info!("found epic:{app}");
        out.push(DetectedApp {
            id: format!("epic:{app}"),
            name,
            source: AppSource::Epic,
            executable: exe,
            install_dir: dir,
            icon_path: None,
            missing: false,
        });
    }
}

fn riot(root: &Path, out: &mut Vec<DetectedApp>) {
    let candidates = [
        (
            "riot:valorant",
            "Valorant",
            root.join(r"VALORANT\live\VALORANT.exe"),
        ),
        (
            "riot:league",
            "League of Legends",
            root.join(r"League of Legends\LeagueClient.exe"),
        ),
        (
            "riot:client",
            "Riot Client",
            root.join(r"Riot Client\RiotClientServices.exe"),
        ),
    ];
    for (id, name, exe) in candidates {
        if exe.exists() {
            tracing::info!("found {id}");
            out.push(DetectedApp {
                id: id.into(),
                name: name.into(),
                source: AppSource::Riot,
                install_dir: exe.parent().unwrap_or(root).to_path_buf(),
                executable: exe,
                icon_path: None,
                missing: false,
            });
        }
    }
}

fn discord(root: &Path, out: &mut Vec<DetectedApp>) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    let mut best: Option<PathBuf> = None;
    for ent in rd.flatten() {
        let p = ent.path();
        let Some(name) = p.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if !name.starts_with("app-") {
            continue;
        }
        let exe = p.join("Discord.exe");
        if exe.exists() {
            best = Some(match best {
                Some(prev) if prev < exe => exe.max(prev),
                Some(prev) => prev.max(exe),
                None => exe,
            });
        }
    }
    // versioned folders: pick lexicographically latest app-*
    if best.is_none() {
        let mut apps: Vec<PathBuf> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|n| n.starts_with("app-"))
            })
            .collect();
        apps.sort();
        if let Some(p) = apps.last() {
            let exe = p.join("Discord.exe");
            if exe.exists() {
                best = Some(exe);
            }
        }
    }
    if let Some(exe) = best {
        let tag = root
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("Discord");
        tracing::info!("found discord:{tag}");
        out.push(DetectedApp {
            id: format!("discord:{tag}"),
            name: tag.to_string(),
            source: AppSource::Discord,
            install_dir: exe.parent().unwrap_or(root).to_path_buf(),
            executable: exe,
            icon_path: None,
            missing: false,
        });
    }
}

fn battlenet(agent: &Path, out: &mut Vec<DetectedApp>) {
    // Agent product.db is binary; look for InstallLocation-like folders nearby.
    let data = agent.join("data");
    let Ok(rd) = std::fs::read_dir(&data) else {
        tracing::info!("skip Battle.net agent data");
        return;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.extension().and_then(|s| s.to_str()) != Some("db") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&p) else {
            continue;
        };
        // UTF-16LE / ASCII path scrape
        let text = String::from_utf8_lossy(&bytes);
        for cand in extract_windows_paths(&text) {
            let exe = if cand.is_file() {
                cand.clone()
            } else {
                match find_exe(&cand) {
                    Some(e) => e,
                    None => continue,
                }
            };
            let name = cand
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Battle.net game")
                .to_string();
            let id = format!("battlenet:{}", name.to_ascii_lowercase().replace(' ', "_"));
            if out.iter().any(|a| a.id == id) {
                continue;
            }
            tracing::info!("found {id}");
            out.push(DetectedApp {
                id,
                name,
                source: AppSource::BattleNet,
                install_dir: exe.parent().unwrap_or(&cand).to_path_buf(),
                executable: exe,
                icon_path: None,
                missing: false,
            });
        }
    }
}

fn gog(config: &Path, out: &mut Vec<DetectedApp>) {
    let Ok(raw) = std::fs::read_to_string(config) else {
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        tracing::info!("skip GOG config.json (parse)");
        return;
    };
    let paths = v
        .pointer("/libraryPath")
        .and_then(|x| x.as_str())
        .into_iter()
        .map(PathBuf::from)
        .chain(
            v.pointer("/installationPaths")
                .and_then(|x| x.as_array())
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(PathBuf::from)),
        );
    for lib in paths {
        let Ok(rd) = std::fs::read_dir(&lib) else {
            continue;
        };
        for ent in rd.flatten() {
            let dir = ent.path();
            if !dir.is_dir() {
                continue;
            }
            let Some(exe) = find_exe(&dir) else { continue };
            let name = dir
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("GOG game")
                .to_string();
            tracing::info!("found gog:{name}");
            out.push(DetectedApp {
                id: format!("gog:{name}"),
                name,
                source: AppSource::Gog,
                executable: exe,
                install_dir: dir,
                icon_path: None,
                missing: false,
            });
        }
    }
}

fn extract_windows_paths(s: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for chunk in s.split(|c: char| c == '\0' || c.is_ascii_control()) {
        if chunk.len() > 6 && chunk.as_bytes().get(1) == Some(&b':') {
            let p = PathBuf::from(chunk.trim());
            if p.exists() {
                out.push(p);
            }
        }
    }
    out
}

fn find_exe(dir: &Path) -> Option<PathBuf> {
    let skip = [
        "uninstall",
        "crash",
        "redist",
        "vcredist",
        "unitycrash",
        "easyanticheat",
        "beclient",
    ];
    let mut best: Option<(i32, PathBuf)> = None;
    let Ok(rd) = std::fs::read_dir(dir) else {
        return None;
    };
    for ent in rd.flatten() {
        let p = ent.path();
        if p.is_dir() {
            continue;
        }
        if p.extension().and_then(|s| s.to_str()).map(|e| e.eq_ignore_ascii_case("exe")) != Some(true)
        {
            continue;
        }
        let name = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if skip.iter().any(|s| name.contains(s)) {
            continue;
        }
        let score = name.len() as i32;
        match &best {
            Some((s, _)) if *s >= score => {}
            _ => best = Some((score, p)),
        }
    }
    best.map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn steam_fixture() {
        let tmp = std::env::temp_dir().join(format!("agg-steam-{}", std::process::id()));
        let apps = tmp.join("steamapps");
        let common = apps.join("common").join("Counter-Strike Global Offensive");
        fs::create_dir_all(&common).unwrap();
        fs::write(common.join("cs2.exe"), b"").unwrap();
        fs::write(
            apps.join("appmanifest_730.acf"),
            r#"
"AppState"
{
	"appid"		"730"
	"name"		"Counter-Strike 2"
	"installdir"		"Counter-Strike Global Offensive"
}
"#,
        )
        .unwrap();
        fs::write(
            apps.join("appmanifest_1.acf"),
            r#"
"AppState"
{
	"appid"		"1"
	"name"		"Some Soundtrack"
	"installdir"		"ost"
}
"#,
        )
        .unwrap();
        let mut out = Vec::new();
        steam_library(&tmp, &mut out);
        let _ = fs::remove_dir_all(&tmp);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "steam:730");
        assert_eq!(out[0].name, "Counter-Strike 2");
    }

    #[test]
    fn epic_fixture() {
        let tmp = std::env::temp_dir().join(format!("agg-epic-{}", std::process::id()));
        let game = tmp.join("Fortnite");
        fs::create_dir_all(&game).unwrap();
        fs::write(game.join("FortniteClient-Win64-Shipping.exe"), b"").unwrap();
        let dat = tmp.join("LauncherInstalled.dat");
        fs::write(
            &dat,
            format!(
                r#"{{"InstallationList":[{{"AppName":"Fortnite","InstallLocation":{}}}]}}"#,
                serde_json::to_string(&game.to_string_lossy().to_string()).unwrap()
            ),
        )
        .unwrap();
        let mut out = Vec::new();
        epic(&dat, &mut out);
        let _ = fs::remove_dir_all(&tmp);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].id, "epic:Fortnite");
    }
}
