use crate::{
    error::{Error, Result},
    plan::{PlanNode::SeqScan, QueryPlan},
    tables::Tuple,
    transaction::Transaction,
};
pub struct Executor<'a> {
    txn: &'a Transaction<'a>,
}
impl Executor<'_> {
    pub fn new<'a>(txn: &'a Transaction<'a>) -> Executor<'a> {
        Executor { txn }
    }

    #[allow(clippy::collapsible_match)] // shut up clippy i'm not done yet
    pub fn execute(&self, plan: QueryPlan) -> Result<Vec<Tuple>> {
        let tuples = match plan {
            QueryPlan::Select(node) => match node {
                SeqScan { table, filter } => self.txn.scan(table.table_id, filter)?,
                _ => return Err(Error::Unknown),
            },
            _ => return Err(Error::Unknown),
        };
        Ok(tuples)
    }
}
