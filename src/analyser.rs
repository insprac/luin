use std::collections::{HashMap, HashSet};

use full_moon::{
    ast::{
        Assignment, Ast, Block, Expression, FunctionArgs, FunctionCall, FunctionDeclaration,
        LocalAssignment, LocalFunction, Parameter, Prefix, Var,
    },
    node::Node,
    tokenizer::{Position, Token, TokenReference},
    visitors::Visitor,
};
use tracing::warn;

#[derive(Debug)]
pub struct LuaAnalysis {
    global_vars: Vec<VariableDefinition>,
    global_usages: HashMap<String, Vec<Position>>,
}

impl LuaAnalysis {
    pub fn from_ast(ast: &Ast) -> Self {
        let mut visitor = LuaAnalyserVisitor::new();
        visitor.visit_ast(ast);
        Self {
            global_vars: visitor.global_vars,
            global_usages: visitor.global_usages,
        }
    }
}

#[derive(Debug)]
pub struct VariableDefinition {
    name: String,
    assign_positions: Vec<Position>,
}

#[derive(Debug)]
struct Scope {
    local_vars: HashSet<String>,
    parent: Option<usize>,
}

#[derive(Debug)]
struct LuaAnalyserVisitor {
    global_vars: Vec<VariableDefinition>,
    global_usages: HashMap<String, Vec<Position>>,
    scopes: Vec<Scope>,
    current_scope: Option<usize>,
}

impl LuaAnalyserVisitor {
    fn new() -> Self {
        Self {
            global_vars: Vec::new(),
            global_usages: HashMap::new(),
            scopes: Vec::new(),
            current_scope: None,
        }
    }

    fn add_global_var(&mut self, name: String, position: Option<Position>) {
        // Ignore if this a reassignment of a local variable
        if self.is_local(&name) {
            return;
        }
        // If a registered definition matches they are merged
        for existing_def in &mut self.global_vars {
            if existing_def.name != name {
                continue;
            }
            if let Some(pos) = position {
                existing_def.assign_positions.push(pos);
            }
            return;
        }
        // If there is no matching definition add a new one
        self.global_vars.push(VariableDefinition {
            name,
            assign_positions: match position {
                Some(pos) => vec![pos],
                None => vec![],
            },
        });
    }

    fn add_local_var(&mut self, name: String) {
        let scope_index = self.current_scope.expect("current scope isn't set");
        let scope = self
            .scopes
            .get_mut(scope_index)
            .expect("current scope doesn't exist");
        scope.local_vars.insert(name);
    }

    fn add_global_usage(&mut self, name: String, position: Position) {
        // Ignore local variables
        if self.is_local(&name) {
            return;
        }
        self.global_usages
            .entry(name)
            .and_modify(|usages| usages.push(position))
            .or_insert(vec![position]);
    }

    fn enter_scope(&mut self) {
        self.scopes.push(Scope {
            local_vars: HashSet::new(),
            parent: self.current_scope,
        });
        self.current_scope = Some(self.scopes.len() - 1);
    }

    fn exit_scope(&mut self) {
        let scope_index = self.current_scope.expect("current scope isn't set");
        let Some(scope) = self.scopes.get(scope_index) else {
            warn!(
                scope_index = self.current_scope,
                "Attempting to exit scope, but current scope not found"
            );
            return;
        };
        let Some(parent_scope) = scope.parent else {
            warn!(
                ?scope,
                "Attempting to exit scope, but current scope has no parent"
            );
            return;
        };
        self.current_scope = Some(parent_scope);
    }

    fn is_local(&mut self, name: &str) -> bool {
        let mut scope_index = self.current_scope.expect("current scope isn't set");
        loop {
            let scope = self.scopes.get(scope_index).expect("scope doesn't exist");
            if scope.local_vars.contains(name) {
                return true;
            }
            let Some(parent_scope) = scope.parent else {
                break;
            };
            scope_index = parent_scope;
        }
        false
    }
}

impl Visitor for LuaAnalyserVisitor {
    fn visit_assignment(&mut self, assignment: &Assignment) {
        for var in assignment.variables() {
            let Var::Name(name_token) = var else {
                // Skip expression assignment for now e.g. `x.y = 123` and `x.y.z() = 321`
                continue;
            };
            let name = name_token.token().to_string().trim().to_owned();
            if !self.is_local(&name) {
                self.add_global_var(name, var.start_position());
            }
        }
    }

    fn visit_local_assignment(&mut self, local_assign: &LocalAssignment) {
        for name in local_assign.names() {
            self.add_local_var(name.to_string().trim().to_owned());
        }
    }

    fn visit_block(&mut self, _node: &Block) {
        self.enter_scope();
    }

    fn visit_block_end(&mut self, _node: &Block) {
        self.exit_scope();
    }

    fn visit_function_declaration(&mut self, func_dec: &FunctionDeclaration) {
        for param in func_dec.body().parameters() {
            if let Parameter::Name(name) = param {
                self.add_local_var(name.token().to_string().trim().to_owned());
            }
        }
    }

    fn visit_local_function(&mut self, local_func: &LocalFunction) {
        self.add_local_var(local_func.name().token().to_string().trim().to_owned());
        for param in local_func.body().parameters() {
            if let Parameter::Name(name) = param {
                self.add_local_var(name.token().to_string().trim().to_owned());
            }
        }
    }

    fn visit_expression(&mut self, expr: &Expression) {
        match expr {
            Expression::Var(var) => match var {
                Var::Name(token_ref) => {
                    let token = token_ref.token();
                    self.add_global_usage(
                        token.to_string().trim().to_owned(),
                        token.start_position(),
                    );
                }
                Var::Expression(_var_expr) => {
                    todo!();
                }
                _ => {}
            },
            Expression::FunctionCall(func_call) => {
                if let Some(position) = func_call.start_position() {
                    self.add_global_usage(
                        func_call.prefix().to_string().trim().to_owned(),
                        position,
                    );
                }
            }
            _ => {}
        }
    }

    fn visit_function_call(&mut self, func_call: &FunctionCall) {
        let Some(position) = func_call.start_position() else {
            // If there is no position we can't provide useful data so ignore it
            return;
        };
        self.add_global_usage(func_call.prefix().to_string().trim().to_owned(), position);
    }
}

#[cfg(test)]
mod tests {
    use crate::analyser::LuaAnalyserVisitor;

    use super::LuaAnalysis;
    use full_moon::{parse, visitors::Visitor};

    #[test]
    fn indexes_global_defs() {
        let ast = parse("x = 123").unwrap();
        let analysis = LuaAnalysis::from_ast(&ast);
        assert_eq!(analysis.global_vars.len(), 1);
        assert_eq!(&analysis.global_vars[0].name, "x");
    }

    #[test]
    fn ignores_local_defs_and_expr_assigns() {
        let code = r#"
        local x = 1
        y = 2
        z.b.a = 3
        z.c = 4
        "#;
        let ast = parse(code).unwrap();
        let analysis = LuaAnalysis::from_ast(&ast);
        assert_eq!(analysis.global_vars.len(), 1);
        assert_eq!(&analysis.global_vars[0].name, "y");
    }

    #[test]
    fn include_nested_assigns() {
        let code = r#"
        function highest(n1, n2)
            if n1 > n2 then
                y = n1;
                return n1;
            else
                y = n2;
                return n2;
            end
        end
        "#;
        let ast = parse(code).unwrap();
        let analysis = LuaAnalysis::from_ast(&ast);
        assert_eq!(analysis.global_vars.len(), 1);
        assert_eq!(&analysis.global_vars[0].name, "y");
    }

    #[test]
    fn reassigned_locals_are_not_globals() {
        let code = r#"local x = 1;
        local y = 2;
        x = 3;
        function test()
            x = 4;
            y = 5;
            z = x;
        end
        "#;
        let ast = parse(code).unwrap();
        let mut visitor = LuaAnalyserVisitor::new();
        visitor.visit_ast(&ast);
        assert_eq!(visitor.global_vars.len(), 1);
        assert_eq!(&visitor.global_vars[0].name, "z");
    }

    #[test]
    fn function_arguments_are_locals() {
        let code = r#"
        function with_args(n1, n2)
            n1 = 1;
            n2 = 2;
        end
        "#;
        let ast = parse(code).unwrap();
        let analysis = LuaAnalysis::from_ast(&ast);
        assert!(analysis.global_vars.is_empty());
    }

    #[test]
    fn includes_all_global_usages() {
        let code = r#"
        x = 1;
        y = 2;
        local z = 3;
        function example()
            z = x + y;
            print(x);
            z = x;
        end
        "#;
        let ast = parse(code).unwrap();
        let analysis = LuaAnalysis::from_ast(&ast);
        assert_eq!(analysis.global_usages.keys().len(), 3);
        assert_eq!(analysis.global_usages.get("x").unwrap().len(), 3);
        assert_eq!(analysis.global_usages.get("y").unwrap().len(), 1);
        assert_eq!(analysis.global_usages.get("print").unwrap().len(), 1);
    }

    #[test]
    fn local_function_usages_are_ignored() {
        let code = r#"
        local function example(y)
            print(y);
        end
        example();
        "#;
        let ast = parse(code).unwrap();
        let analysis = LuaAnalysis::from_ast(&ast);
        assert!(analysis.global_vars.is_empty());
        assert_eq!(analysis.global_usages.keys().len(), 1);
        assert_eq!(analysis.global_usages.get("print").unwrap().len(), 1);
    }
}
