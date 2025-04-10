use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs,
    path::Path,
};
use syn::{ImplItem, Item, ItemFn, parse_file, visit::Visit};

#[derive(Default)]
pub struct FunctionCallVisitor {
    pub current_function: String,
    pub current_module: Vec<String>,
    pub function_calls: Vec<(String, String)>,
    pub functions: HashMap<String, syn::ItemFn>,
    pub qualified_functions: HashMap<String, syn::ItemFn>,
    pub struct_methods: HashMap<String, syn::ImplItemFn>,
    pub impl_blocks: HashMap<String, Vec<syn::ImplItemFn>>, // Track implementations by type
    pub visited_files: HashSet<String>,
    pub current_call_stack: Vec<String>, // Track call stack to handle recursion
}

impl FunctionCallVisitor {
    fn get_qualified_name(&self, name: &str) -> String {
        if self.current_module.is_empty() {
            name.to_string()
        } else {
            format!("{}::{}", self.current_module.join("::"), name)
        }
    }

    pub fn process_function(&mut self, name: &str) {
        if self.current_call_stack.contains(&name.to_string()) {
            return; // Prevent infinite recursion
        }

        self.current_call_stack.push(name.to_string());

        if let Some(func) = self.functions.get(name).cloned() {
            let old_function = self.current_function.clone();
            self.current_function = name.to_string();

            syn::visit::visit_item_fn(self, &func);

            self.current_function = old_function;
        }

        self.current_call_stack.pop();
    }

    fn process_method(&mut self, type_name: &str, method_name: &str) {
        let qualified_method = format!("{}::{}", type_name, method_name);

        if self.current_call_stack.contains(&qualified_method) {
            return;
        }

        self.current_call_stack.push(qualified_method.clone());

        // Clone the method before processing to avoid borrow conflicts
        let method_to_process = self.impl_blocks.get(type_name).and_then(|impls| {
            impls
                .iter()
                .find(|method| method.sig.ident.to_string() == method_name)
                .cloned()
        });

        if let Some(method) = method_to_process {
            let old_function = self.current_function.clone();
            self.current_function = qualified_method;

            syn::visit::visit_impl_item_fn(self, &method);

            self.current_function = old_function;
        }

        self.current_call_stack.pop();
    }

    fn process_impl_block(&mut self, impl_block: &syn::ItemImpl) -> Result<(), Box<dyn Error>> {
        let type_name = match &*impl_block.self_ty {
            syn::Type::Path(type_path) => {
                type_path.path.segments.last().map(|s| s.ident.to_string())
            }
            _ => None,
        };

        if let Some(type_name) = type_name {
            let mut methods = Vec::new();

            for item in &impl_block.items {
                if let ImplItem::Fn(method) = item {
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
                    methods.push(method.clone());
                }
            }

            self.impl_blocks.insert(type_name, methods);
        }

        Ok(())
    }

    pub fn process_module(&mut self, module_path: &Path) -> Result<(), Box<dyn Error>> {
        let canon_path = module_path.canonicalize()?;
        let path_str = canon_path.to_string_lossy().to_string();

        if self.visited_files.contains(&path_str) {
            return Ok(());
        }
        self.visited_files.insert(path_str);

        let content = fs::read_to_string(module_path)?;
        let syntax = parse_file(&content)?;

        let module_name = module_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();

        if module_name != "main" && module_name != "lib" {
            self.current_module.push(module_name);
        }

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
                        let mod_name = module.ident.to_string();
                        self.current_module.push(mod_name);

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

        if !self.current_module.is_empty() {
            self.current_module.pop();
        }

        Ok(())
    }
}

impl<'ast> Visit<'ast> for FunctionCallVisitor {
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(path) = &*call.func {
            let callee = path.path.segments.last().map(|s| s.ident.to_string());

            if let Some(callee) = callee {
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

                if self.functions.contains_key(&callee)
                    || self.qualified_functions.contains_key(&qualified_callee)
                    || self.struct_methods.contains_key(&qualified_callee)
                {
                    let caller = self.get_qualified_name(&self.current_function);
                    self.function_calls
                        .push((caller.clone(), qualified_callee.clone()));

                    if self.functions.contains_key(&callee) {
                        self.process_function(&callee);
                    } else if let Some(parts) = qualified_callee.rsplit_once("::") {
                        self.process_method(parts.0, parts.1);
                    }
                }
            }
        } else if let syn::Expr::MethodCall(method_call) = &*call.func {
            let method_name = method_call.method.to_string();

            // Find matching method first, collecting necessary information
            let method_info = self
                .impl_blocks
                .iter()
                .find(|(_, methods)| methods.iter().any(|m| m.sig.ident == method_call.method))
                .map(|(struct_name, _)| struct_name.clone());

            if let Some(struct_name) = method_info {
                let qualified_method = format!("{}::{}", struct_name, method_name);
                let caller = self.get_qualified_name(&self.current_function);
                self.function_calls
                    .push((caller.clone(), qualified_method.clone()));
                self.process_method(&struct_name, &method_name);
            }
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, method_call: &'ast syn::ExprMethodCall) {
        let method_name = method_call.method.to_string();

        // Find matching method first, collecting necessary information
        let method_info = self
            .impl_blocks
            .iter()
            .find(|(_, methods)| methods.iter().any(|m| m.sig.ident == method_call.method))
            .map(|(struct_name, _)| struct_name.clone());

        // Process the method if found
        if let Some(struct_name) = method_info {
            let qualified_method = format!("{}::{}", struct_name, method_name);
            let caller = self.get_qualified_name(&self.current_function);
            self.function_calls
                .push((caller.clone(), qualified_method.clone()));
            self.process_method(&struct_name, &method_name);
        }

        // Continue visiting the receiver and arguments
        syn::visit::visit_expr_method_call(self, method_call);
    }
}
