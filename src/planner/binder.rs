use crate::database::Database;
use crate::error::Result;
use crate::{
    error::Error::ParseError,
    expr::Expr::{self, AttrRef},
    planner::planner::SqlStatement,
    tables::Table,
    value::Value,
};
use sqlparser::ast::TableFactor;
use sqlparser::ast::{
    self, Query,
    SelectItem::{self},
    SetExpr::Select,
    Statement, TableWithJoins,
};

pub struct Binder {
    sql_parser_statement: Option<Statement>,
}

impl Binder {
    pub fn new() -> Self {
        Self {
            sql_parser_statement: None,
        }
    }
    pub fn bind(&mut self, statement: Statement, db: &Database) -> Result<SqlStatement> {
        self.sql_parser_statement = Some(statement);
        let my_statement = match &self.sql_parser_statement {
            Some(Statement::Query(query)) => {
                let Query { body, .. } = &**query;
                match &**body {
                    Select(select_query) => self.parse_select_ast(select_query, db)?,
                    _ => todo!(),
                }
            }
            _ => todo!(),
        };
        Ok(my_statement)
    }
    fn parse_select_ast(&self, select_ast: &ast::Select, db: &Database) -> Result<SqlStatement> {
        let ast::Select {
            projection, // columns
            from,       // vec of tables
            selection,  // optional filter expression tree
            ..
        } = select_ast;
        let tables = self.parse_from(from, db)?;
        let cols = self.parse_projection(projection, &tables)?;
        let filter = self.parse_filter(selection)?;
        Ok(SqlStatement::Select {
            cols,
            tables,
            filter,
        })
    }
    fn parse_projection(&self, projection: &[SelectItem], tables: &[Table]) -> Result<Vec<Expr>> {
        let mut exprs = vec![];
        for proj in projection {
            match proj {
                SelectItem::Wildcard(_) => return Ok(vec![]),
                SelectItem::UnnamedExpr(ast_exp) => {
                    if let ast::Expr::Identifier(ident) = ast_exp {
                        // Validate with schema (need table/schema here)
                        let mut matched = false;
                        for table in tables {
                            let schema = &table.schema;
                            for (i, col) in schema.attributes.iter().enumerate() {
                                if col.name == ident.value {
                                    matched = true;
                                    exprs.push(AttrRef(i));
                                }
                            }
                        }
                        if !matched {
                            return Err(ParseError(format!("col {} not found!", ident.value)));
                        }
                    } else if let ast::Expr::Value(val_span) = ast_exp {
                        let expr = match &val_span.value {
                            ast::Value::Null => Ok(Expr::Constant(Value::Null)),
                            ast::Value::Boolean(b) => Ok(Expr::Constant(Value::Boolean(*b))),
                            ast::Value::Number(num, _wtf) => {
                                if let Ok(int) = num.parse::<i32>() {
                                    Ok(Expr::Constant(Value::Int(int)))
                                } else if let Ok(float) = num.parse::<f32>() {
                                    Ok(Expr::Constant(Value::Float(float)))
                                } else {
                                    Err(ParseError(format!("couldn't bind {} to a datatype!", num)))
                                }
                            }
                            ast::Value::SingleQuotedString(str)
                            | ast::Value::DoubleQuotedString(str) => {
                                Ok(Expr::Constant(Value::VarChar(str.to_string())))
                            }
                            _ => {
                                todo!()
                            }
                        };
                        exprs.push(expr?);
                    } else if let ast::Expr::BinaryOp { left, op, right } = ast_exp {
                        // TODO: Write recursive function to convert left, right, op into our own
                        // BinarOp type to traverse this tree.
                        todo!()
                    }
                }
                _ => unimplemented!(),
            }
        }
        Ok(exprs)
    }

    fn parse_expr(&self, expr: Box<ast::Expr>) -> Expr {
        let ret_expr = match *expr {
            ast::Expr::BinaryOp { left, op, right } => match op {
                ast::BinaryOperator::Plus => Expr::Add(
                    Box::new(self.parse_expr(left)),
                    Box::new(self.parse_expr(right)),
                ),
                ast::BinaryOperator::Eq => Expr::Equal(
                    Box::new(self.parse_expr(left)),
                    Box::new(self.parse_expr(right)),
                ),
                _ => {
                    todo!()
                }
            },
            _ => {
                println!("{:?}", expr);
                todo!()
            }
        };
        ret_expr
    }

    fn parse_from(&self, from: &[TableWithJoins], db: &Database) -> Result<Vec<Table>> {
        let mut tables = vec![];
        for table in from {
            match &table.relation {
                TableFactor::Table { name, .. } => {
                    if let Some(table) = db.get_table(&name.to_string()) {
                        tables.push(table.clone());
                    }
                }
                _ => {
                    unimplemented!()
                }
            }
        }
        Ok(tables)
    }
    fn parse_filter(&self, selection: &Option<ast::Expr>) -> Result<Option<Expr>> {
        if let Some(expression) = selection {
            self.parse_expr(Box::new(expression.clone()));
        }
        Ok(None)
    }
}

impl Default for Binder {
    fn default() -> Self {
        Self::new()
    }
}
