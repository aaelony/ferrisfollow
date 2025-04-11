use std::{error::Error, process::Command};

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

// Helper function to format module paths
pub fn format_module_path(components: &[String]) -> String {
    components.join("::")
}

// Helper function to extract crate name from a fully qualified path
pub fn extract_crate_name(path: &str) -> &str {
    path.split("::").next().unwrap_or("")
}

// Helper function to determine if a path is a test module
pub fn is_test_module(path: &str) -> bool {
    path.contains("tests") || path.contains("test_") || path.ends_with("_test")
}

// Helper function to determine if a path is an example
pub fn is_example_module(path: &str) -> bool {
    path.contains("examples") || path.contains("example_") || path.ends_with("_example")
}
