use crate::{
    INITIAL_FIRST_FREE_PAGE_NUMBER, PageId,
    database::Database,
    error::{DbError, DbResult},
    expr::Expr,
    ids::TableId,
    page::{PageKind, SlottedPage},
    tables::{Table, Tuple},
    value::Value,
};

pub struct SeqScanExecutor<'a> {
    txn: &'a Transaction<'a>,
    filter: &'a Option<Expr>,
    current_page: u64,
    current_slot: u16,
    num_pages: Option<u64>,
    table: &'a Table,
}
impl<'a> SeqScanExecutor<'a> {
    pub fn new(txn: &'a Transaction, table_id: TableId, filter: &'a Option<Expr>) -> Self {
        let table = txn.db.tables.get(&table_id).unwrap();
        let num_pages = txn
            .db
            .buffer_manager
            .storage_manager
            .borrow()
            .get_num_pages(table.file_id);
        Self {
            txn,
            filter,
            current_page: INITIAL_FIRST_FREE_PAGE_NUMBER,
            current_slot: 0,
            num_pages,
            table,
        }
    }

    pub fn next_tuple(&mut self) -> DbResult<Option<Tuple>> {
        while self.current_page < self.num_pages.unwrap() {
            tracing::warn!(
                "Page: {}, NumPages: {:?}",
                self.current_page,
                self.num_pages
            );
            let guard = self.txn.db.buffer_manager.get_page(PageId {
                file_id: self.table.file_id,
                page_num: self.current_page,
            })?;
            if guard.kind() != PageKind::Heap {
                tracing::warn!("Not a heap page, continuing");
                self.current_page += 1;
                continue;
            }
            let tuple = guard.with_heap(|heap| {
                let num_entries = heap.num_entries();
                while self.current_slot < num_entries {
                    tracing::warn!("Slot: {}, NumSlots: {}", self.current_slot, num_entries);
                    let bytes = heap.get_slot(self.current_slot)?;
                    if bytes.is_none() {
                        return Ok(None);
                    }
                    let tuple = Tuple::deserialize(bytes.unwrap(), &self.table.schema)?;
                    if let Some(predicate) = self.filter {
                        match predicate.evaluate(Some(&tuple))? {
                            Value::Boolean(true) => {
                                self.current_slot += 1;
                                return Ok(Some(tuple));
                            }
                            Value::Boolean(false) | Value::Null => {}
                            other => {
                                return Err(DbError::InvalidComparison(format!(
                                    "filter returned {other:?}"
                                )));
                            }
                        }
                    } else {
                        self.current_slot += 1;
                        return Ok(Some(tuple));
                    }
                    self.current_slot += 1;
                }
                Ok(None)
            })?;
            if tuple.is_none() {
                self.current_page += 1;
                self.current_slot = 0;
            } else {
                return Ok(tuple);
            }
        }
        Ok(None)
    }
}

pub struct Transaction<'a> {
    pub db: &'a mut Database,
}
impl Transaction<'_> {
    pub fn new(db: &mut Database) -> Transaction<'_> {
        Transaction { db }
    }

    pub fn scan(&self, table: TableId, filter: Option<Expr>) -> DbResult<Vec<Tuple>> {
        let mut tuples = vec![];
        let mut scan = SeqScanExecutor::new(self, table, &filter);
        while let Some(tuple) = scan.next_tuple()? {
            tuples.push(tuple);
        }
        Ok(tuples)
    }
}
