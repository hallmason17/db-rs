use crate::{
    database::Database, error::Result, execution::seq_scan::SeqScanExecutor, expr::Expr,
    ids::TableId, tables::Tuple,
};

pub struct Transaction<'a> {
    pub db: &'a mut Database,
}
impl Transaction<'_> {
    pub fn new(db: &mut Database) -> Transaction<'_> {
        Transaction { db }
    }

    pub fn scan(&self, table: TableId, filter: Option<Expr>) -> Result<Vec<Tuple>> {
        let mut tuples = vec![];
        let mut scan = SeqScanExecutor::new(self, table, &filter);
        while let Some(tuple) = scan.next_tuple()? {
            tuples.push(tuple);
        }
        Ok(tuples)
    }
}
