use crate::{
    expr::Expr,
    tables::{Table, TableSchema},
};

pub enum SortOrder {
    Ascending,
    Descending,
}

pub enum QueryPlan {
    CreateTable { schema: TableSchema },
    Update,
    Delete,
    Select(PlanNode),
}

pub enum PlanNode {
    Filter {
        data_source: Box<PlanNode>,
        pred: Expr,
    },
    SeqScan {
        table: Table,
        filter: Option<Expr>,
    },
    Sort {
        data_source: Box<PlanNode>,
        order: SortOrder,
    },
}
