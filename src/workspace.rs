use crate::{
    cargo::WorkspaceAnalyzer,
    visitor::{FunctionCallVisitor, function::FunctionProcessor, module::ModuleProcessor},
};
use petgraph::{Graph, prelude::*};
use std::{
    collections::HashMap,
    error::Error,
    path::{Path, PathBuf},
};

pub struct WorkspaceAnalysis {
    pub root_path: PathBuf,
    pub visitors: Vec<FunctionCallVisitor>,
    pub crate_names: HashMap<String, PathBuf>,
}

impl WorkspaceAnalysis {
    pub fn new(path: &Path) -> Result<Self, Box<dyn Error>> {
        let workspace = WorkspaceAnalyzer::new(path)?;
        let mut analysis = WorkspaceAnalysis {
            root_path: path.to_path_buf(),
            visitors: Vec::new(),
            crate_names: HashMap::new(),
        };

        analysis.analyze_workspace(&workspace)?;
        Ok(analysis)
    }

    fn analyze_workspace(&mut self, workspace: &WorkspaceAnalyzer) -> Result<(), Box<dyn Error>> {
        println!("Starting workspace analysis...");
        for entry_point in workspace.get_entry_points() {
            println!("Processing entry point: {:?}", entry_point);
            let mut visitor = FunctionCallVisitor::default();

            if let Some(crate_path) = entry_point.parent().and_then(|p| p.parent()) {
                let crate_name = crate_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();

                println!("Found crate: {}", crate_name);
                self.crate_names
                    .insert(crate_name, crate_path.to_path_buf());
            }

            visitor.process_module(&entry_point)?;
            println!(
                "Found {} function calls in this module",
                visitor.function_calls.len()
            );
            self.visitors.push(visitor);
        }

        Ok(())
    }

    pub fn create_combined_graph(&self) -> Graph<String, usize, Directed> {
        let mut combined_graph = Graph::new();
        let mut node_indices = HashMap::new();
        let mut edge_sequence = 1;

        // Create all nodes
        for visitor in &self.visitors {
            for (caller, callee) in &visitor.function_calls {
                for func in [caller, callee] {
                    if !node_indices.contains_key(func) {
                        let idx = combined_graph.add_node(func.clone());
                        node_indices.insert(func.clone(), idx);
                    }
                }
            }
        }

        // Add edges with sequence numbers
        for visitor in &self.visitors {
            for (caller, callee) in &visitor.function_calls {
                if let (Some(&caller_idx), Some(&callee_idx)) =
                    (node_indices.get(caller), node_indices.get(callee))
                {
                    combined_graph.add_edge(caller_idx, callee_idx, edge_sequence);
                    edge_sequence += 1;
                }
            }
        }

        combined_graph
    }

    pub fn get_entry_points(&self) -> Vec<String> {
        self.visitors
            .iter()
            .flat_map(|v| v.function_calls.iter())
            .filter(|(caller, _)| caller.ends_with("::main"))
            .map(|(caller, _)| caller.clone())
            .collect()
    }

    pub fn get_crate_info(&self) -> &HashMap<String, PathBuf> {
        &self.crate_names
    }

    pub fn get_cross_crate_calls(&self) -> Vec<(String, String)> {
        self.visitors
            .iter()
            .flat_map(|v| v.function_calls.iter())
            .filter(|(caller, callee)| {
                let caller_crate = caller.split("::").next().unwrap_or("");
                let callee_crate = callee.split("::").next().unwrap_or("");
                caller_crate != callee_crate
            })
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    pub include_tests: bool,
    pub include_examples: bool,
    pub max_depth: Option<usize>,
    pub include_external_crates: bool,
    pub start_functions: Vec<String>,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        AnalysisConfig {
            include_tests: false,
            include_examples: false,
            max_depth: None,
            include_external_crates: false,
            start_functions: vec!["main".to_string()],
        }
    }
}

pub fn analyze_repository(
    path: &Path,
    config: AnalysisConfig,
) -> Result<WorkspaceAnalysis, Box<dyn Error>> {
    let mut analysis = WorkspaceAnalysis::new(path)?;

    // Process each start function
    for func in &config.start_functions {
        for visitor in &mut analysis.visitors {
            visitor.process_function(func);
        }
    }

    Ok(analysis)
}
