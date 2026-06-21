/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use crate::{
    error::{Error, Result},
    execution::{create_table::CreateTableExecutor, insert::InsertExecutor},
    planner::plan::{PlanNode::SeqScan, QueryPlan},
    tables::Tuple,
    transaction::Transaction,
};
pub struct Executor<'a> {
    txn: &'a mut Transaction<'a>,
}
impl Executor<'_> {
    pub fn new<'a>(txn: &'a mut Transaction<'a>) -> Executor<'a> {
        Executor { txn }
    }

    #[allow(clippy::collapsible_match)]
    pub fn execute(&mut self, plan: QueryPlan) -> Result<Vec<Tuple>> {
        tracing::debug!("Executing plan: {:?}", plan);
        let tuples = match plan {
            QueryPlan::Select(node) => match node {
                SeqScan {
                    table,
                    cols,
                    filter,
                } => self.txn.scan(table.table_id, &cols, filter)?,
                _ => return Err(Error::Unknown),
            },
            QueryPlan::Insert { table, values } => {
                let vals: Vec<_> = values
                    .iter()
                    .map(|e| e.evaluate(None))
                    .collect::<Result<_>>()?;
                let tuple = Tuple::new(vals);
                let record = tuple.serialize(&table.schema);
                let mut insert_executor =
                    InsertExecutor::new(&mut *self.txn.db, table.table_id, record);
                insert_executor.execute()?;
                vec![]
            }
            QueryPlan::CreateTable { name, schema } => {
                let mut create_table_executor =
                    CreateTableExecutor::new(self.txn.db, &name, &schema);
                create_table_executor.execute()?;
                vec![]
            }
            _ => return Err(Error::Unknown),
        };
        Ok(tuples)
    }
}
