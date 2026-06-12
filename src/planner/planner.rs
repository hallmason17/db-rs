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
}

pub enum SortOrder {
    Ascending,
    Descending,
}

pub enum QueryPlan {
    CreateTable { schema: TableSchema },
    Insert,
    Update,
    Delete,
    Select(PlanNode),
}

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
        }
    }
}
