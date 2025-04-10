use petgraph::{
    Graph,
    dot::{Config, Dot},
    prelude::*,
};
use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs,
    path::Path,
    process::Command,
};
use syn::{Item, ItemFn, parse_file, visit::Visit};
use walkdir::WalkDir;

#[derive(Default)]
struct FunctionCallVisitor {
    current_function: String,
    current_module: Vec<String>,
    function_calls: Vec<(String, String)>,
    functions: HashMap<String, syn::ItemFn>,
    qualified_functions: HashMap<String, syn::ItemFn>,
    struct_methods: HashMap<String, syn::ImplItemFn>, // Changed to ImplItemFn
}

impl FunctionCallVisitor {
    fn process_lib_file(&mut self, lib_path: &Path) -> Result<(), Box<dyn Error>> {
        let content = fs::read_to_string(lib_path)?;
        let syntax = parse_file(&content)?;

        // First collect all module declarations
        let mut modules = Vec::new();
        for item in syntax.items {
            match item {
                Item::Mod(module) => {
                    modules.push(module.ident.to_string());
                }
                _ => {}
            }
        }

        // Process each module
        for module_name in modules {
            // Check both possible module locations
            let module_path = lib_path.parent().unwrap().join(&module_name).join("mod.rs");
            let direct_path = lib_path
                .parent()
                .unwrap()
                .join(format!("{}.rs", module_name));

            if module_path.exists() {
                self.current_module.push(module_name.clone());
                self.process_module(&module_path)?;
                self.current_module.pop();
            } else if direct_path.exists() {
                self.current_module.push(module_name.clone());
                self.process_module(&direct_path)?;
                self.current_module.pop();
            }
        }

        Ok(())
    }

    fn get_qualified_name(&self, name: &str) -> String {
        if self.current_module.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", self.current_module.join("::"), name)
        }
    }

    fn process_function(&mut self, name: &str) {
        if let Some(func) = self.functions.get(name).cloned() {
            let old_function = self.current_function.clone();
            self.current_function = name.to_string();

            syn::visit::visit_item_fn(self, &func);

            self.current_function = old_function;
        }
    }

    fn process_module(&mut self, module_path: &Path) -> Result<(), Box<dyn Error>> {
        let content = fs::read_to_string(module_path)?;
        let syntax = parse_file(&content)?;

        // Process all items in the module
        for item in syntax.items {
            match item {
                Item::Fn(func) => {
                    let name = func.sig.ident.to_string();
                    let qualified_name = self.get_qualified_name(&name);
                    self.functions.insert(name.clone(), func.clone());
                    self.qualified_functions.insert(qualified_name, func);
                }
                Item::Impl(impl_block) => {
                    self.process_impl_block(&impl_block)?;
                }
                Item::Mod(module) => {
                    if let Some((_, items)) = module.content {
                        self.current_module.push(module.ident.to_string());
                        for item in items {
                            match item {
                                Item::Fn(func) => {
                                    let name = func.sig.ident.to_string();
                                    let qualified_name = self.get_qualified_name(&name);
                                    self.functions.insert(name.clone(), func.clone());
                                    self.qualified_functions.insert(qualified_name, func);
                                }
                                Item::Impl(impl_block) => {
                                    self.process_impl_block(&impl_block)?;
                                }
                                _ => {}
                            }
                        }
                        self.current_module.pop();
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    fn process_impl_block(&mut self, impl_block: &syn::ItemImpl) -> Result<(), Box<dyn Error>> {
        // Get the type name for the impl block
        let type_name = if let syn::Type::Path(type_path) = &*impl_block.self_ty {
            type_path.path.segments.last().map(|s| s.ident.to_string())
        } else {
            None
        };

        if let Some(type_name) = type_name {
            for item in &impl_block.items {
                if let syn::ImplItem::Fn(method) = item {
                    let method_name = method.sig.ident.to_string();
                    let qualified_name = if self.current_module.is_empty() {
                        format!("{}::{}", type_name, method_name)
                    } else {
                        format!(
                            "{}::{}::{}",
                            self.current_module.join("::"),
                            type_name,
                            method_name
                        )
                    };
                    self.struct_methods.insert(qualified_name, method.clone());
                }
            }
        }

        Ok(())
    }
}

impl<'ast> Visit<'ast> for FunctionCallVisitor {
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = &*call.func {
            let callee = path.path.segments.last().map(|s| s.ident.to_string());

            if let Some(callee) = callee {
                // Try to resolve the full path
                let qualified_callee = if path.path.segments.len() > 1 {
                    path.path
                        .segments
                        .iter()
                        .map(|s| s.ident.to_string())
                        .collect::<Vec<_>>()
                        .join("::")
                } else {
                    self.get_qualified_name(&callee)
                };

                // Check if it's a function or method we care about
                if self.functions.contains_key(&callee)
                    || self.qualified_functions.contains_key(&qualified_callee)
                    || self.struct_methods.contains_key(&qualified_callee)
                {
                    let caller = self.get_qualified_name(&self.current_function);
                    self.function_calls.push((caller, qualified_callee));
                }
            }
        } else if let syn::Expr::MethodCall(method_call) = &*call.func {
            // Handle method calls
            let method_name = method_call.method.to_string();
            // Try to resolve the type and create qualified method name
            // This is a simplified version - might need more sophisticated type resolution
            if let Some(qualified_method) = self
                .struct_methods
                .keys()
                .find(|k| k.ends_with(&format!("::{}", method_name)))
            {
                let caller = self.get_qualified_name(&self.current_function);
                self.function_calls.push((caller, qualified_method.clone()));
            }
        }
        syn::visit::visit_expr_call(self, call);
    }
}

fn analyze_directory(dir: &Path) -> Result<FunctionCallVisitor, Box<dyn Error>> {
    let mut visitor = FunctionCallVisitor::default();

    // First process lib.rs to understand the module structure
    let lib_path = dir.join("src/lib.rs");
    if lib_path.exists() {
        visitor.process_lib_file(&lib_path)?;
    }

    // Process main.rs
    let main_path = dir.join("src/main.rs");
    if main_path.exists() {
        visitor.process_module(&main_path)?;
    }

    // Process all other .rs files that might not be declared in lib.rs
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

fn create_call_graph(visitor: &FunctionCallVisitor) -> Graph<String, usize, Directed> {
    let mut graph = Graph::new();
    let mut node_indices = HashMap::new();

    // Create nodes
    let mut seen_functions = HashSet::new();
    for (caller, callee) in &visitor.function_calls {
        seen_functions.insert(caller);
        seen_functions.insert(callee);
    }

    for func in seen_functions {
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

fn write_dot_file(
    graph: &Graph<String, usize, Directed>,
    filename: &str,
) -> Result<(), Box<dyn Error>> {
    let mut file = fs::File::create(filename)?;
    use std::io::Write;

    // Extended Flowbite color palette
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

    // First pass: determine node colors based on incoming edges
    for e in graph.edge_indices() {
        let (_, to) = graph.edge_endpoints(e).unwrap();
        let sequence = graph.edge_weight(e).unwrap();
        let color_index =
            ((sequence - 1) as f32 * (colors.len() - 1) as f32 / (num_calls - 1) as f32) as usize;
        node_colors.insert(to, colors[color_index]);
    }

    // Add nodes with colors
    for i in graph.node_indices() {
        let color = node_colors.get(&i).unwrap_or(&"black");
        writeln!(
            file,
            "    {} [label=\"{}\", color=\"{}\", penwidth=2.0];",
            i.index(),
            graph[i].replace("\"", ""),
            color
        )?;
    }

    writeln!(file)?;

    // Add edges with colors
    for e in graph.edge_indices() {
        let (from, to) = graph.edge_endpoints(e).unwrap();
        let sequence = graph.edge_weight(e).unwrap();

        let color_index =
            ((sequence - 1) as f32 * (colors.len() - 1) as f32 / (num_calls - 1) as f32) as usize;
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

    writeln!(file, "}}")?;

    Ok(())
}

fn generate_png(dot_file: &str, png_file: &str) -> Result<(), Box<dyn Error>> {
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

fn check_graphviz_installed() -> bool {
    Command::new("dot")
        .arg("-V")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn main() -> Result<(), Box<dyn Error>> {
    let dir = Path::new(".");
    let visitor = analyze_directory(dir)?;
    let graph = create_call_graph(&visitor);
    let dot_file = "call_graph.dot";
    let png_file = "call_graph.png";

    write_dot_file(&graph, dot_file)?;
    println!("Generated call graph in '{}'", dot_file);

    if !check_graphviz_installed() {
        println!("Warning: Graphviz (dot) is not installed. Only DOT file will be generated.");
        println!("Install Graphviz to automatically generate PNG visualizations.");
        return Ok(());
    }

    match generate_png(dot_file, png_file) {
        Ok(_) => println!("Generated PNG visualization in '{}'", png_file),
        Err(e) => println!("Failed to generate PNG: {}. Is Graphviz installed?", e),
    }

    Ok(())
}
