use crate::visitor::FunctionCallVisitor;
use petgraph::{
    Graph,
    //  dot::{Config, Dot},
    prelude::*,
};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs::File,
    io::Write,
};

fn simplify_name(name: &str) -> String {
    // Remove our internal workspace-related prefixes
    name.replace("WorkspaceAnalysis::", "")
        .replace("WorkspaceAnalyzer::", "")
}

pub fn create_call_graph(visitor: &FunctionCallVisitor) -> Graph<String, usize, Directed> {
    let mut graph = Graph::new();
    let mut node_indices = HashMap::new();

    // Create nodes
    let mut seen_functions = HashSet::new();
    for (caller, callee) in &visitor.function_calls {
        seen_functions.insert(caller.clone());
        seen_functions.insert(callee.clone());
    }

    // Sort functions to ensure consistent node ordering
    let mut functions: Vec<_> = seen_functions.into_iter().collect();
    functions.sort();

    for func in functions {
        let idx = graph.add_node(func.clone());
        node_indices.insert(func, idx);
    }

    // Create edges with sequence numbers
    for (sequence, (caller, callee)) in visitor.function_calls.iter().enumerate() {
        if let (Some(&caller_idx), Some(&callee_idx)) =
            (node_indices.get(caller), node_indices.get(callee))
        {
            graph.add_edge(caller_idx, callee_idx, sequence + 1);
        }
    }

    graph
}

pub fn write_dot_file(
    graph: &Graph<String, usize, Directed>,
    filename: &str,
) -> Result<(), Box<dyn Error>> {
    let mut file = File::create(filename)?;

    let colors = [
        // Blues
        "#1e40af", // Blue-800
        "#1d4ed8", // Blue-700
        "#2563eb", // Blue-600
        "#3b82f6", // Blue-500
        "#60a5fa", // Blue-400
        // Purples
        "#6d28d9", // Purple-700
        "#7c3aed", // Purple-600
        "#8b5cf6", // Purple-500
        "#a78bfa", // Purple-400
        // Pinks
        "#be185d", // Pink-700
        "#db2777", // Pink-600
        "#ec4899", // Pink-500
        "#f472b6", // Pink-400
        // Greens
        "#166534", // Green-700
        "#16a34a", // Green-600
        "#22c55e", // Green-500
        "#4ade80", // Green-400
        "#86efac", // Green-300
        // Yellows
        "#facc15", // Yellow-400
        "#fbbf24", // Yellow-400
        "#f59e0b", // Yellow-500
        // Oranges
        "#ea580c", // Orange-600
        "#f97316", // Orange-500
        "#fb923c", // Orange-400
        // Reds
        "#dc2626", // Red-600
        "#b91c1c", // Red-700
        "#991b1b", // Red-800
    ];

    writeln!(file, "digraph {{")?;
    writeln!(file, "    node [shape=box];\n")?;

    let num_calls = graph.edge_count();

    // Create a map to store the last incoming edge color for each node
    let mut node_colors: HashMap<NodeIndex, &str> = HashMap::new();

    // Track unique edges to avoid duplicates
    let mut seen_edges: HashSet<(NodeIndex, NodeIndex)> = HashSet::new();
    // First pass: determine node colors based on incoming edges
    for e in graph.edge_indices() {
        let (from, to) = graph.edge_endpoints(e).unwrap();
        if seen_edges.insert((from, to)) {
            // Only process if this is a new edge
            let sequence = graph.edge_weight(e).unwrap();
            let color_index = ((sequence - 1) as f32 * (colors.len() - 1) as f32
                / (num_calls - 1) as f32) as usize;
            node_colors.insert(to, colors[color_index]);
        }
    }

    // Add nodes with colors
    for i in graph.node_indices() {
        let color = node_colors.get(&i).unwrap_or(&"black");
        writeln!(
            file,
            "    {} [label=\"{}\", color=\"{}\", penwidth=2.0];",
            i.index(),
            simplify_name(&graph[i]).replace("\"", ""),
            color
        )?;
    }

    writeln!(file)?;

    // Reset seen edges for edge writing
    seen_edges.clear();

    // Add edges with colors
    for e in graph.edge_indices() {
        let (from, to) = graph.edge_endpoints(e).unwrap();
        if seen_edges.insert((from, to)) {
            // Only write if this is a new edge
            let sequence = graph.edge_weight(e).unwrap();

            let color_index = ((sequence - 1) as f32 * (colors.len() - 1) as f32
                / (num_calls - 1) as f32) as usize;
            let color = colors[color_index];

            writeln!(
                file,
                "    {} -> {} [label=\"{}\", color=\"{}\", fontcolor=\"{}\", penwidth=2.0];",
                from.index(),
                to.index(),
                sequence,
                color,
                color
            )?;
        }
    }

    writeln!(file, "}}")?;

    Ok(())
}
