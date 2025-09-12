use std::collections::HashSet;

use full_moon::{
    ast::{Assignment, Ast, Block, Do, LocalAssignment, Var},
    node::Node,
    tokenizer::Position,
    visitors::Visitor,
};
use tracing::warn;

#[derive(Debug)]
pub struct VariableDefinition {
    name: String,
    assign_positions: Vec<Position>,
}

#[derive(Debug)]
pub struct Scope {
    local_vars: HashSet<String>,
    parent: Option<usize>,
}

pub struct LuaAnalysis {
    global_vars: Vec<VariableDefinition>,
}

impl LuaAnalysis {
    pub fn from_ast(ast: &Ast) -> Self {
        let mut visitor = LuaAnalyserVisitor::new();
        visitor.visit_ast(ast);
        Self {
            global_vars: visitor.global_vars,
        }
    }
}

pub struct LuaAnalyserVisitor {
    global_vars: Vec<VariableDefinition>,
    scopes: Vec<Scope>,
    current_scope: usize,
}

impl LuaAnalyserVisitor {
    pub fn new() -> Self {
        let mut analyser = Self {
            global_vars: Vec::new(),
            scopes: Vec::new(),
            current_scope: 0,
        };

        // Add the initial scope which all nested scopes will inherit
        analyser.scopes.push(Scope {
            local_vars: HashSet::new(),
            parent: None,
        });

        analyser
    }

    fn add_global_var(&mut self, name: String, position: Option<Position>) {
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

    fn enter_scope(&mut self) {
        self.scopes.push(Scope {
            local_vars: HashSet::new(),
            parent: Some(self.current_scope),
        });
        self.current_scope = self.scopes.len() - 1;
    }

    fn exit_scope(&mut self) {
        let Some(scope) = self.scopes.get(self.current_scope) else {
            warn!(
                scope_id = self.current_scope,
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
        self.current_scope = parent_scope;
    }
}

impl Visitor for LuaAnalyserVisitor {
    fn visit_assignment(&mut self, assignment: &Assignment) {
        for var in assignment.variables() {
            let Var::Name(name_token) = var else {
                // Skip expression assignment for now e.g. `x.y = 123` and `x.y.z() = 321`
                continue;
            };
            let name_string = name_token.token().to_string();
            self.add_global_var(name_string, var.start_position());
        }
    }

    fn visit_block(&mut self, _node: &Block) {
        self.enter_scope();
    }

    fn visit_block_end(&mut self, _node: &Block) {
        self.exit_scope();
    }

    fn visit_local_assignment(&mut self, local_assign: &LocalAssignment) {
        for name in local_assign.names() {
            let scope = self
                .scopes
                .get_mut(self.current_scope)
                .expect("Current scope doesn't exist");
            scope.local_vars.insert(name.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LuaAnalysis;
    use full_moon::parse;

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
}
