/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use crate::{
    database::Database,
    error::Result,
    execution::insert::InsertExecutor,
    execution::seq_scan::SeqScanExecutor,
    expr::Expr,
    ids::TableId,
    tables::{RecordId, Tuple},
};

pub struct Transaction<'a> {
    pub db: &'a mut Database,
}
impl Transaction<'_> {
    pub fn new(db: &mut Database) -> Transaction<'_> {
        tracing::info!("Starting new transaction on DB");
        Transaction { db }
    }

    pub fn scan(&self, table: TableId, cols: &[Expr], filter: Option<Expr>) -> Result<Vec<Tuple>> {
        tracing::debug!(
            "Scanning table ID {:?} with cols {:?} and filter {:?}",
            table,
            cols,
            filter
        );
        let mut tuples = vec![];
        let mut scan = SeqScanExecutor::new(self, table, cols, &filter);
        while let Some(tuple) = scan.next_tuple()? {
            tuples.push(tuple);
        }
        Ok(tuples)
    }

    pub fn insert(&mut self, table_id: TableId, record: &[u8]) -> Result<RecordId> {
        tracing::debug!(
            "Inserting into table ID {:?} record len {:?}",
            table_id,
            record.len()
        );
        InsertExecutor::new(&mut *self.db, table_id, record.to_vec()).execute()
    }
}
