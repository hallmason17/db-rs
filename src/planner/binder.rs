/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use crate::{
    database::Database,
    error::{Error, Result},
    expr::Expr,
    planner::plan::SqlStatement,
    tables::{ColumnDefinition, Table, TableSchema},
    value::{DataType, Value},
};
use sqlparser::ast::{
    self, BinaryOperator, ColumnDef, Ident, Query, SelectItem, SetExpr, Statement, TableFactor,
    TableWithJoins, UnaryOperator,
};

pub struct Binder;

impl Binder {
    pub fn new() -> Self {
        Self
    }

    pub fn bind(&mut self, statement: Statement, db: &Database) -> Result<SqlStatement> {
        match statement {
            Statement::Query(query) => {
                let Query { body, .. } = *query;
                match *body {
                    SetExpr::Select(select_query) => self.parse_select_ast(&select_query, db),
                    other => Err(Error::ParseError(format!(
                        "unsupported query expression: {other:?}"
                    ))),
                }
            }
            Statement::Insert(insert) => {
                let table_name = insert.table.to_string();
                let table = db
                    .get_table(&table_name)
                    .cloned()
                    .ok_or(Error::TableNotFound(table_name))?;

                let query = insert
                    .source
                    .ok_or(Error::ParseError("INSERT requires a VALUES clause".into()))?;
                let tables = vec![table.clone()];
                let values = match *query.body {
                    SetExpr::Values(values) => {
                        let row = values.rows.first().ok_or(Error::ParseError(
                            "INSERT requires at least one row of values".into(),
                        ))?;
                        row.content
                            .iter()
                            .map(|e| self.parse_expr(e, &tables))
                            .collect::<Result<Vec<_>>>()?
                    }
                    _ => {
                        return Err(Error::ParseError(
                            "only INSERT ... VALUES is supported".into(),
                        ));
                    }
                };

                Ok(SqlStatement::Insert { table, values })
            }
            Statement::CreateTable(create) => {
                let name = create.name.to_string();
                let columns = create
                    .columns
                    .iter()
                    .map(|e| self.parse_col(e))
                    .collect::<Result<Vec<_>>>()?;
                let schema = TableSchema::new(&columns);
                tracing::debug!("CREATE TABLE {}, schema: \n{:?}", name, schema);
                Ok(SqlStatement::CreateTable { name, schema })
            }
            other => Err(Error::ParseError(format!(
                "unsupported SQL statement: {other:?}"
            ))),
        }
    }

    fn parse_col(&self, col_def: &ColumnDef) -> Result<ColumnDefinition> {
        let datatype = match &col_def.data_type {
            ast::DataType::Varchar(_) => DataType::VarChar,
            ast::DataType::Int32 | ast::DataType::Int(_) => DataType::Int,
            ast::DataType::Float32 | ast::DataType::Float(_) => DataType::Float,
            ast::DataType::Blob(_) | ast::DataType::Binary(_) => DataType::Blob,
            other => {
                return Err(Error::ParseError(format!(
                    "Datatype {} not supported",
                    other
                )));
            }
        };
        let _is_key = col_def
            .options
            .iter()
            .any(|o| matches!(o.option, ast::ColumnOption::PrimaryKey(_)));
        let is_nullable = !col_def
            .options
            .iter()
            .any(|o| matches!(o.option, ast::ColumnOption::NotNull));
        ColumnDefinition::new(col_def.name.clone().to_string(), datatype, is_nullable)
    }

    fn parse_select_ast(&self, select_ast: &ast::Select, db: &Database) -> Result<SqlStatement> {
        let ast::Select {
            projection,
            from,
            selection,
            ..
        } = select_ast;

        let tables = self.parse_from(from, db)?;
        let cols = self.parse_projection(projection, &tables)?;
        let filter = self.parse_filter(selection, &tables)?;

        Ok(SqlStatement::Select {
            cols,
            tables,
            filter,
        })
    }

    fn parse_projection(&self, projection: &[SelectItem], tables: &[Table]) -> Result<Vec<Expr>> {
        let mut exprs = vec![];
        for item in projection {
            match item {
                SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => return Ok(vec![]),
                SelectItem::UnnamedExpr(expr) => exprs.push(self.parse_expr(expr, tables)?),
                SelectItem::ExprWithAlias { expr, .. }
                | SelectItem::ExprWithAliases { expr, .. } => {
                    exprs.push(self.parse_expr(expr, tables)?)
                }
            }
        }
        Ok(exprs)
    }

    fn parse_filter(
        &self,
        selection: &Option<ast::Expr>,
        tables: &[Table],
    ) -> Result<Option<Expr>> {
        selection
            .as_ref()
            .map(|expression| self.parse_expr(expression, tables))
            .transpose()
    }

    fn parse_expr(&self, expr: &ast::Expr, tables: &[Table]) -> Result<Expr> {
        match expr {
            ast::Expr::BinaryOp { left, op, right } => {
                self.parse_binary_expr(left, op, right, tables)
            }
            ast::Expr::CompoundIdentifier(idents) => self.resolve_compound_column(idents, tables),
            ast::Expr::Identifier(ident) => self.resolve_column(None, &ident.value, tables),
            ast::Expr::IsNotNull(expr) => {
                Ok(Expr::IsNotNull(Box::new(self.parse_expr(expr, tables)?)))
            }
            ast::Expr::IsNull(expr) => Ok(Expr::IsNull(Box::new(self.parse_expr(expr, tables)?))),
            ast::Expr::Nested(expr) => self.parse_expr(expr, tables),
            ast::Expr::UnaryOp { op, expr } => self.parse_unary_expr(op, expr, tables),
            ast::Expr::Value(value) => self.parse_value(&value.value),
            other => Err(Error::ParseError(format!(
                "unsupported expression: {other:?}"
            ))),
        }
    }

    fn parse_binary_expr(
        &self,
        left: &ast::Expr,
        op: &BinaryOperator,
        right: &ast::Expr,
        tables: &[Table],
    ) -> Result<Expr> {
        let left = || self.parse_expr(left, tables).map(Box::new);
        let right = || self.parse_expr(right, tables).map(Box::new);
        let eq = || Ok(Expr::Equal(left()?, right()?));

        match op {
            BinaryOperator::And => Ok(Expr::And(left()?, right()?)),
            BinaryOperator::Divide => Ok(Expr::Divide(left()?, right()?)),
            BinaryOperator::Eq => eq(),
            BinaryOperator::Gt => Ok(Expr::GreaterThan(left()?, right()?)),
            BinaryOperator::GtEq => Ok(Expr::Or(
                Box::new(Expr::GreaterThan(left()?, right()?)),
                Box::new(eq()?),
            )),
            BinaryOperator::Lt => Ok(Expr::LessThan(left()?, right()?)),
            BinaryOperator::LtEq => Ok(Expr::Or(
                Box::new(Expr::LessThan(left()?, right()?)),
                Box::new(eq()?),
            )),
            BinaryOperator::Minus => Ok(Expr::Subtract(left()?, right()?)),
            BinaryOperator::Multiply => Ok(Expr::Multiply(left()?, right()?)),
            BinaryOperator::NotEq => Ok(Expr::Not(Box::new(eq()?))),
            BinaryOperator::Or => Ok(Expr::Or(left()?, right()?)),
            BinaryOperator::Plus => Ok(Expr::Add(left()?, right()?)),
            other => Err(Error::ParseError(format!(
                "unsupported binary operator: {other:?}"
            ))),
        }
    }

    fn parse_unary_expr(
        &self,
        op: &UnaryOperator,
        expr: &ast::Expr,
        tables: &[Table],
    ) -> Result<Expr> {
        match op {
            UnaryOperator::BangNot | UnaryOperator::Not => {
                Ok(Expr::Not(Box::new(self.parse_expr(expr, tables)?)))
            }
            UnaryOperator::Minus => Ok(Expr::Subtract(
                Box::new(Expr::Constant(Value::Int(0))),
                Box::new(self.parse_expr(expr, tables)?),
            )),
            UnaryOperator::Plus => self.parse_expr(expr, tables),
            other => Err(Error::ParseError(format!(
                "unsupported unary operator: {other:?}"
            ))),
        }
    }

    fn parse_value(&self, value: &ast::Value) -> Result<Expr> {
        let value = match value {
            ast::Value::Boolean(value) => Value::Boolean(*value),
            ast::Value::DoubleQuotedString(value) | ast::Value::SingleQuotedString(value) => {
                Value::VarChar(value.as_str().into())
            }
            ast::Value::Null => Value::Null,
            ast::Value::Number(value, _) => {
                if let Ok(value) = value.parse::<i32>() {
                    Value::Int(value)
                } else if let Ok(value) = value.parse::<f32>() {
                    Value::Float(value)
                } else {
                    return Err(Error::ParseError(format!(
                        "couldn't bind {value} to a datatype"
                    )));
                }
            }
            other => {
                return Err(Error::ParseError(format!(
                    "unsupported literal value: {other:?}"
                )));
            }
        };

        Ok(Expr::Constant(value))
    }

    fn resolve_compound_column(&self, idents: &[Ident], tables: &[Table]) -> Result<Expr> {
        match idents {
            [column] => self.resolve_column(None, &column.value, tables),
            [table, column] => self.resolve_column(Some(&table.value), &column.value, tables),
            _ => Err(Error::ParseError(format!(
                "unsupported compound identifier: {}",
                idents
                    .iter()
                    .map(|ident| ident.value.as_str())
                    .collect::<Vec<_>>()
                    .join(".")
            ))),
        }
    }

    fn resolve_column(
        &self,
        table_name: Option<&str>,
        column_name: &str,
        tables: &[Table],
    ) -> Result<Expr> {
        let mut matched_attr = None;

        for table in tables {
            if table_name.is_some_and(|name| name != table.name) {
                continue;
            }

            for (idx, col) in table.schema.attributes.iter().enumerate() {
                if col.name == column_name {
                    if matched_attr.is_some() {
                        return Err(Error::ParseError(format!(
                            "ambiguous column `{column_name}`"
                        )));
                    }
                    matched_attr = Some(idx);
                }
            }
        }

        matched_attr.map(Expr::AttrRef).ok_or_else(|| {
            if let Some(table_name) = table_name {
                Error::ParseError(format!("column `{table_name}.{column_name}` not found"))
            } else {
                Error::ParseError(format!("column `{column_name}` not found"))
            }
        })
    }

    fn parse_from(&self, from: &[TableWithJoins], db: &Database) -> Result<Vec<Table>> {
        if from.len() != 1 {
            return Err(Error::ParseError(
                "select currently requires exactly one FROM table".into(),
            ));
        }

        let table_with_joins = &from[0];
        if !table_with_joins.joins.is_empty() {
            return Err(Error::ParseError("joins are not implemented yet".into()));
        }

        match &table_with_joins.relation {
            TableFactor::Table { name, .. } => {
                let table_name = name.to_string();
                db.get_table(&table_name)
                    .cloned()
                    .ok_or(Error::TableNotFound(table_name))
                    .map(|table| vec![table])
            }
            other => Err(Error::ParseError(format!(
                "unsupported FROM relation: {other:?}"
            ))),
        }
    }
}

impl Default for Binder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        buffer_pool::{BufferPool, ReplacementStrategy},
        error::Error,
        storage::StorageManager,
        tables::{ColumnDefinition, TableSchema, Tuple},
        value::{DataType, Value},
    };
    use sqlparser::{dialect::GenericDialect, parser::Parser};
    use tempfile::tempdir;

    fn setup_db() -> (tempfile::TempDir, Database) {
        let dir = tempdir().unwrap();
        let sm = StorageManager::new(dir.path()).unwrap();
        let bp = BufferPool::new(16, ReplacementStrategy::Clock, sm).unwrap();
        let mut db = Database::open(dir.path().into(), bp).unwrap();
        let schema = TableSchema::new(&[
            ColumnDefinition::new("id".into(), DataType::Int, false).unwrap(),
            ColumnDefinition::new("name".into(), DataType::VarChar, true).unwrap(),
        ]);
        db.create_table("users", &schema).unwrap();
        (dir, db)
    }

    fn bind(sql: &str, db: &Database) -> Result<SqlStatement> {
        let statement = Parser::parse_sql(&GenericDialect {}, sql)
            .unwrap()
            .remove(0);
        Binder::new().bind(statement, db)
    }

    fn only_select(sql: &str, db: &Database) -> (Vec<Expr>, Option<Expr>) {
        match bind(sql, db).unwrap() {
            SqlStatement::Select { cols, filter, .. } => (cols, filter),
            _ => panic!("expected Select statement"),
        }
    }

    #[test]
    fn binds_binary_projection_expression() {
        let (_dir, db) = setup_db();
        let (cols, _) = only_select("select id + 1 from users", &db);
        let tuple = Tuple::new(vec![Value::Int(41), Value::VarChar("mason".into())]);

        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].evaluate(Some(&tuple)).unwrap(), Value::Int(42));
    }

    #[test]
    fn binds_filter_expression() {
        let (_dir, db) = setup_db();
        let (_, filter) = only_select("select id from users where id >= 10 and name <> 'bob'", &db);
        let tuple = Tuple::new(vec![Value::Int(10), Value::VarChar("mason".into())]);

        assert_eq!(
            filter.unwrap().evaluate(Some(&tuple)).unwrap(),
            Value::Boolean(true)
        );
    }

    #[test]
    fn binds_table_qualified_column() {
        let (_dir, db) = setup_db();
        let (cols, _) = only_select("select users.id from users", &db);

        assert!(matches!(cols.as_slice(), [Expr::AttrRef(0)]));
    }

    #[test]
    fn binds_null_checks_and_not() {
        let (_dir, db) = setup_db();
        let (_, filter) = only_select("select id from users where not (name is null)", &db);
        let tuple = Tuple::new(vec![Value::Int(1), Value::VarChar("mason".into())]);

        assert_eq!(
            filter.unwrap().evaluate(Some(&tuple)).unwrap(),
            Value::Boolean(true)
        );
    }

    #[test]
    fn missing_table_returns_error() {
        let (_dir, db) = setup_db();
        let err = bind("select id from missing", &db).unwrap_err();

        assert!(matches!(err, Error::TableNotFound(table) if table == "missing"));
    }

    #[test]
    fn missing_column_returns_error() {
        let (_dir, db) = setup_db();
        let err = bind("select missing from users", &db).unwrap_err();

        assert!(matches!(err, Error::ParseError(message) if message.contains("missing")));
    }
}
