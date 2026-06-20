/* Copyright (C) 2026 Mason Hall.
 *
 * This file is part of db-rs.
 *
 * db-rs is free software: you can redistribute it and/or modify it under the
 * terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or (at your option) any later
 * version.
 *
 * db-rs is distributed in the hope that it will be useful, but WITHOUT ANY
 * WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
 * details.
 *
 * You should have received a copy of the GNU General Public License along with
 * db-rs. If not, see <https://www.gnu.org/licenses/>.
 */
use crate::{
    error::{Error, Result},
    execution::insert::InsertExecutor,
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
            _ => return Err(Error::Unknown),
        };
        Ok(tuples)
    }
}
