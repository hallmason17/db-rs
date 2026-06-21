/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use crate::{
    expr::Expr,
    tables::{Table, TableSchema},
};

#[derive(Debug)]
pub enum SqlStatement {
    Select {
        cols: Vec<Expr>,
        tables: Vec<Table>,
        filter: Option<Expr>,
    },
    Insert {
        table: Table,
        values: Vec<Expr>,
    },
    CreateTable {
        name: String,
        schema: TableSchema,
    },
}

#[derive(Debug)]
pub enum SortOrder {
    Ascending,
    Descending,
}

#[derive(Debug)]
pub enum QueryPlan {
    CreateTable { name: String, schema: TableSchema },
    Insert { table: Table, values: Vec<Expr> },
    Update,
    Delete,
    Select(PlanNode),
}

#[derive(Debug)]
pub enum PlanNode {
    Filter {
        children: Vec<PlanNode>,
        pred: Expr,
    },
    SeqScan {
        cols: Vec<Expr>,
        table: Table,
        filter: Option<Expr>,
    },
    Sort {
        children: Vec<PlanNode>,
        order: SortOrder,
    },
}

pub struct Planner {}
impl Planner {
    pub fn new() -> Self {
        Self {}
    }
    pub fn plan(&self, statement: SqlStatement) -> QueryPlan {
        tracing::debug!("Planning statement: {:?}", statement);
        match statement {
            SqlStatement::Select {
                cols,
                tables,
                filter,
            } => QueryPlan::Select(PlanNode::SeqScan {
                cols,
                table: tables.first().unwrap().clone(),
                filter,
            }),
            SqlStatement::Insert { table, values } => QueryPlan::Insert { table, values },
            SqlStatement::CreateTable { name, schema } => QueryPlan::CreateTable { name, schema },
        }
    }
}
impl Default for Planner {
    fn default() -> Self {
        Self::new()
    }
}
