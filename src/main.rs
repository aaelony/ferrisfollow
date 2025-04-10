mod graph;
mod utils;
mod visitor;

use std::{error::Error, path::Path};
use walkdir::WalkDir;

fn main() -> Result<(), Box<dyn Error>> {
    let dir = Path::new(".");
    let visitor = utils::analyze_directory(dir)?;
    let graph = graph::create_call_graph(&visitor);

    let dot_file = "call_graph.dot";
    let png_file = "call_graph.png";

    graph::write_dot_file(&graph, dot_file)?;
    println!("Generated call graph in '{}'", dot_file);

    if !utils::check_graphviz_installed() {
        println!("Warning: Graphviz (dot) is not installed. Only DOT file will be generated.");
        println!("Install Graphviz to automatically generate PNG visualizations.");
        return Ok(());
    }

    match utils::generate_png(dot_file, png_file) {
        Ok(_) => println!("Generated PNG visualization in '{}'", png_file),
        Err(e) => println!("Failed to generate PNG: {}. Is Graphviz installed?", e),
    }

    Ok(())
}
