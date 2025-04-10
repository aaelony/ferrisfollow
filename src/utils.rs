use crate::visitor::FunctionCallVisitor;
use std::{error::Error, path::Path, process::Command};
use walkdir::WalkDir;

pub fn generate_png(dot_file: &str, png_file: &str) -> Result<(), Box<dyn Error>> {
    let output = Command::new("dot")
        .arg("-Tpng")
        .arg(dot_file)
        .arg("-o")
        .arg(png_file)
        .output()?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Failed to generate PNG: {}", error).into());
    }

    Ok(())
}

pub fn check_graphviz_installed() -> bool {
    Command::new("dot")
        .arg("-V")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn analyze_directory(dir: &Path) -> Result<FunctionCallVisitor, Box<dyn Error>> {
    let mut visitor = FunctionCallVisitor::default();

    // First, process lib.rs if it exists
    let lib_path = dir.join("src/lib.rs");
    if lib_path.exists() {
        visitor.process_module(&lib_path)?;
    }

    // Then process main.rs
    let main_path = dir.join("src/main.rs");
    if main_path.exists() {
        visitor.process_module(&main_path)?;
    }

    // Process all other .rs files
    for entry in WalkDir::new(dir.join("src"))
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map_or(false, |ext| ext == "rs")
                && e.path()
                    .file_name()
                    .map_or(false, |name| name != "main.rs" && name != "lib.rs")
        })
    {
        visitor.process_module(entry.path())?;
    }

    // Start analysis from main
    visitor.process_function("main");

    Ok(visitor)
}
