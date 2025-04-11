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
    pub struct_methods: HashMap<String, syn::ImplItemFn>,
    pub impl_blocks: HashMap<String, Vec<syn::ImplItemFn>>,
    pub visited_files: HashSet<String>,
    pub current_call_stack: Vec<String>,
}

impl FunctionCallVisitor {
    fn get_qualified_name(&self, name: &str) -> String {
        match (name.contains("::"), self.current_module.is_empty()) {
            (true, _) => name.to_string(),
            (false, true) => name.to_string(),
            (false, false) => format!("{}::{}", self.current_module.join("::"), name),
        }
    }

    pub fn process_function(&mut self, name: &str) {
        let qualified_name = self.get_qualified_name(name);

        if self.current_call_stack.contains(&qualified_name) {
            return; // Prevent infinite recursion
        }

        self.current_call_stack.push(qualified_name.clone());

        match self.functions.get(&qualified_name).cloned() {
            Some(func) => {
                let old_function = self.current_function.clone();
                self.current_function = qualified_name;
                syn::visit::visit_item_fn(self, &func);
                self.current_function = old_function;
            }
            None => (),
        }
        self.current_call_stack.pop();
    }

    fn process_method(&mut self, type_name: &str, method_name: &str) {
        let qualified_method = format!("{}::{}", type_name, method_name);
        if self.current_call_stack.contains(&qualified_method) {
            return;
        }
        self.current_call_stack.push(qualified_method.clone());
        let method_to_process = self.impl_blocks.get(type_name).and_then(|impls| {
            impls
                .iter()
                .find(|method| method.sig.ident.to_string() == method_name)
                .cloned()
        });

        match method_to_process {
            Some(method) => {
                let old_function = self.current_function.clone();
                self.current_function = qualified_method;
                syn::visit::visit_impl_item_fn(self, &method);
                self.current_function = old_function;
            }
            None => (),
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

        match type_name {
            Some(type_name) => {
                let mut methods = Vec::new();

                for item in &impl_block.items {
                    match item {
                        ImplItem::Fn(method) => {
                            let method_name = method.sig.ident.to_string();
                            let qualified_name = match self.current_module.is_empty() {
                                true => format!("{}::{}", type_name, method_name),
                                false => format!(
                                    "{}::{}::{}",
                                    self.current_module.join("::"),
                                    type_name,
                                    method_name
                                ),
                            };
                            self.struct_methods.insert(qualified_name, method.clone());
                            methods.push(method.clone());
                        }
                        _ => (),
                    }
                }

                self.impl_blocks.insert(type_name, methods);
            }
            None => (),
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
                    self.functions.insert(qualified_name, func);
                }
                Item::Impl(impl_block) => {
                    self.process_impl_block(&impl_block)?;
                }
                Item::Mod(module) => match module.content {
                    Some((_, items)) => {
                        let mod_name = module.ident.to_string();
                        self.current_module.push(mod_name);

                        for item in items {
                            match item {
                                Item::Fn(func) => {
                                    let name = func.sig.ident.to_string();
                                    let qualified_name = self.get_qualified_name(&name);
                                    self.functions.insert(qualified_name, func);
                                }
                                Item::Impl(impl_block) => {
                                    self.process_impl_block(&impl_block)?;
                                }
                                _ => (),
                            }
                        }

                        self.current_module.pop();
                    }
                    None => (),
                },
                _ => (),
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
        match &*call.func {
            syn::Expr::Path(path) => match path.path.segments.last().map(|s| s.ident.to_string()) {
                Some(callee) => {
                    let qualified_callee = match path.path.segments.len() > 1 {
                        true => path
                            .path
                            .segments
                            .iter()
                            .map(|s| s.ident.to_string())
                            .collect::<Vec<_>>()
                            .join("::"),
                        false => self.get_qualified_name(&callee),
                    };

                    if self.functions.contains_key(&qualified_callee)
                        || self.struct_methods.contains_key(&qualified_callee)
                    {
                        let caller = self.get_qualified_name(&self.current_function);
                        self.function_calls
                            .push((caller.clone(), qualified_callee.clone()));

                        match qualified_callee.rsplit_once("::") {
                            Some(parts) if !self.functions.contains_key(&qualified_callee) => {
                                self.process_method(parts.0, parts.1);
                            }
                            _ => self.process_function(&qualified_callee),
                        }
                    }
                }
                None => (),
            },
            syn::Expr::MethodCall(method_call) => {
                let method_name = method_call.method.to_string();

                match self
                    .impl_blocks
                    .iter()
                    .find(|(_, methods)| methods.iter().any(|m| m.sig.ident == method_call.method))
                    .map(|(struct_name, _)| struct_name.clone())
                {
                    Some(struct_name) => {
                        let qualified_method = format!("{}::{}", struct_name, method_name);
                        let caller = self.get_qualified_name(&self.current_function);
                        self.function_calls
                            .push((caller.clone(), qualified_method.clone()));
                        self.process_method(&struct_name, &method_name);
                    }
                    None => (),
                }
            }
            _ => (),
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, method_call: &'ast syn::ExprMethodCall) {
        let method_name = method_call.method.to_string();

        match self
            .impl_blocks
            .iter()
            .find(|(_, methods)| methods.iter().any(|m| m.sig.ident == method_call.method))
            .map(|(struct_name, _)| struct_name.clone())
        {
            Some(struct_name) => {
                let qualified_method = format!("{}::{}", struct_name, method_name);
                let caller = self.get_qualified_name(&self.current_function);
                self.function_calls
                    .push((caller.clone(), qualified_method.clone()));
                self.process_method(&struct_name, &method_name);
            }
            None => (),
        }

        syn::visit::visit_expr_method_call(self, method_call);
    }
}
