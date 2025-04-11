mod cargo;
mod graph;
mod utils;
mod visitor;
mod workspace;

use std::{error::Error, path::Path};
use workspace::{AnalysisConfig, analyze_repository};

fn main() -> Result<(), Box<dyn Error>> {
    let dir = Path::new(".");

    let config = AnalysisConfig {
        include_tests: false,
        include_examples: false,
        max_depth: None,
        include_external_crates: false,
        start_functions: vec!["main".to_string()],
    };

    println!("Starting analysis of {:?}", dir);
    let analysis = analyze_repository(dir, config)?;

    let graph = analysis.create_combined_graph();

    let dot_file = "call_graph.dot";
    let png_file = "call_graph.png";

    if analysis.get_crate_info().len() > 1 {
        println!("\nAnalyzing workspace with crates:");
        for (crate_name, path) in analysis.get_crate_info() {
            println!("  {} at {:?}", crate_name, path);
        }

        println!("\nEntry points found:");
        for entry in analysis.get_entry_points() {
            println!("  {}", entry);
        }

        if !analysis.get_cross_crate_calls().is_empty() {
            println!("\nCross-crate calls:");
            for (caller, callee) in analysis.get_cross_crate_calls() {
                println!("  {} -> {}", caller, callee);
            }
        }
    }

    graph::write_dot_file(&graph, dot_file)?;
    println!("\nGenerated call graph in '{}'", dot_file);

    if !utils::check_graphviz_installed() {
        println!("Warning: Graphviz (dot) is not installed. Only DOT file will be generated.");
        println!("Install Graphviz to automatically generate PNG visualizations.");
        return Ok(());
    }

    match utils::generate_png(dot_file, png_file) {
        Ok(_) => println!("Generated PNG visualization in '{}'", png_file),
        Err(e) => println!("Failed to generate PNG: {}. Is Graphviz installed?", e),
    }

    println!("\nAnalysis Summary:");
    println!("  Total functions: {}", graph.node_count());
    println!("  Total calls: {}", graph.edge_count());

    Ok(())
}
