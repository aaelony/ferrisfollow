use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use toml::Value;

#[derive(Debug)]
pub struct CargoConfig {
    pub name: String,
    pub entry_points: Vec<PathBuf>,
    pub src_path: PathBuf,
    pub is_workspace: bool,
    pub dependencies: Vec<String>,
}

impl CargoConfig {
    pub fn from_path(path: &Path) -> Result<Self, Box<dyn Error>> {
        let cargo_toml = path.join("Cargo.toml");
        let content = fs::read_to_string(&cargo_toml)?;
        let value = content.parse::<Value>()?;

        let name = value
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or("unknown")
            .to_string();

        let src_path = path.join("src");

        let mut entry_points = Vec::new();
        if src_path.join("main.rs").exists() {
            entry_points.push(src_path.join("main.rs"));
        }
        if src_path.join("lib.rs").exists() {
            entry_points.push(src_path.join("lib.rs"));
        }

        let dependencies = value
            .get("dependencies")
            .and_then(|d| d.as_table())
            .map(|t| t.keys().cloned().collect())
            .unwrap_or_default();

        let is_workspace = value.get("workspace").is_some();

        Ok(CargoConfig {
            name,
            entry_points,
            src_path,
            is_workspace,
            dependencies,
        })
    }
}

pub struct WorkspaceAnalyzer {
    root_path: PathBuf,
    members: Vec<CargoConfig>,
}

impl WorkspaceAnalyzer {
    pub fn new(path: &Path) -> Result<Self, Box<dyn Error>> {
        let root_path = path.to_path_buf();
        let cargo_toml = path.join("Cargo.toml");
        let content = fs::read_to_string(&cargo_toml)?;
        let value = content.parse::<Value>()?;

        let mut members = Vec::new();

        // Check if this is a workspace
        if let Some(workspace) = value.get("workspace") {
            if let Some(workspace_members) = workspace.get("members").and_then(|m| m.as_array()) {
                for member in workspace_members {
                    if let Some(member_path) = member.as_str() {
                        let full_path = path.join(member_path);
                        if let Ok(config) = CargoConfig::from_path(&full_path) {
                            members.push(config);
                        }
                    }
                }
            }
        } else {
            // Single crate
            if let Ok(config) = CargoConfig::from_path(path) {
                members.push(config);
            }
        }

        Ok(WorkspaceAnalyzer { root_path, members })
    }

    pub fn get_entry_points(&self) -> Vec<PathBuf> {
        self.members
            .iter()
            .flat_map(|m| m.entry_points.clone())
            .collect()
    }

    pub fn is_workspace(&self) -> bool {
        self.members.len() > 1
    }
}
