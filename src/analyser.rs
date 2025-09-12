use full_moon::{
    ast::{Assignment, Ast, Var},
    node::Node,
    tokenizer::Position,
    visitors::Visitor,
};

#[derive(Debug)]
pub struct Definition {
    name: String,
    assign_positions: Vec<Position>,
}

pub struct LuaAnalyser {
    definitions: Vec<Definition>,
}

impl LuaAnalyser {
    pub fn new() -> Self {
        Self {
            definitions: Vec::new(),
        }
    }

    pub fn analyse_ast(&mut self, ast: &Ast) {
        self.visit_ast(ast);
    }

    fn add_definition(&mut self, name: String, position: Option<Position>) {
        // If a registered definition matches they are merged
        for existing_def in &mut self.definitions {
            if existing_def.name != name {
                continue;
            }
            if let Some(pos) = position {
                existing_def.assign_positions.push(pos);
            }
            return;
        }
        // If there is no matching definition add a new one
        self.definitions.push(Definition {
            name,
            assign_positions: match position {
                Some(pos) => vec![pos],
                None => vec![]
            },
        });
    }
}

impl Visitor for LuaAnalyser {
    fn visit_assignment(&mut self, assignment: &Assignment) {
        for var in assignment.variables() {
            let Var::Name(name_token) = var else {
                // Skip expression assignment for now e.g. `x.y = 123` and `x.y.z() = 321`
                continue;
            };
            let name_string = name_token.token().to_string();
            self.add_definition(name_string, var.start_position());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LuaAnalyser;

    fn analyse(code: &str) -> LuaAnalyser {
        let ast = full_moon::parse(code).unwrap();
        let mut analyser = LuaAnalyser::new();
        analyser.analyse_ast(&ast);
        analyser
    }

    #[test]
    fn indexes_global_defs() {
        let analyser = analyse("x = 123");
        assert_eq!(analyser.definitions.len(), 1);
        assert_eq!(&analyser.definitions[0].name, "x");
    }

    #[test]
    fn ignores_local_defs_and_expr_assigns() {
        let code = r#"
        local x = 1
        y = 2
        z.b.a = 3
        z.c = 4
        "#;
        let analyser = analyse(code);
        assert_eq!(analyser.definitions.len(), 1);
        assert_eq!(&analyser.definitions[0].name, "y");
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

        let analyser = analyse(code);
        assert_eq!(analyser.definitions.len(), 1);
        assert_eq!(&analyser.definitions[0].name, "y");
    }
}
