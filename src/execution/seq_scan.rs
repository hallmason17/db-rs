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
use crate::error::Error::InvalidComparison;
use crate::tables::TupleRef;
use crate::value::{Value, ValueRef};
use crate::{
    error::Result,
    expr::Expr,
    ids::{PageId, TableId},
    page::{PageKind, SlottedPage},
    tables::{Table, Tuple},
    transaction::Transaction,
};

pub struct SeqScanExecutor<'a> {
    txn: &'a Transaction<'a>,
    filter: &'a Option<Expr>,
    current_page: u64,
    current_slot: u16,
    num_pages: Option<u64>,
    table: &'a Table,
    cols: &'a [Expr],
}
impl<'a> SeqScanExecutor<'a> {
    pub fn new(
        txn: &'a Transaction,
        table_id: TableId,
        cols: &'a [Expr],
        filter: &'a Option<Expr>,
    ) -> Self {
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
            current_page: 1,
            current_slot: 0,
            num_pages,
            table,
            cols,
        }
    }

    pub fn next_tuple(&mut self) -> Result<Option<Tuple>> {
        while self.current_page < self.num_pages.unwrap() {
            tracing::debug!(
                "Scanning Page: {}, NumPages: {:?}",
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
            let match_output_schema = |tup_ref: &TupleRef| -> Result<Tuple> {
                let vals: Vec<Value> = self
                    .cols
                    .iter()
                    .map(|expr| match expr.evaluate_ref(Some(tup_ref))? {
                        ValueRef::Int(i) => Ok(Value::Int(i)),
                        ValueRef::Float(f) => Ok(Value::Float(f)),
                        ValueRef::Boolean(b) => Ok(Value::Boolean(b)),
                        ValueRef::VarChar(cow) => Ok(Value::VarChar(cow.into_owned().into())),
                        ValueRef::Blob(cow) => Ok(Value::Blob(cow.into_owned().into())),
                        ValueRef::Null => Ok(Value::Null),
                    })
                    .collect::<Result<_>>()?;
                Ok(Tuple::new(vals))
            };
            let tuple = guard.with_heap(|heap| {
                let num_entries = heap.num_entries();
                while self.current_slot < num_entries {
                    let bytes = heap.get_slot(self.current_slot)?;
                    if bytes.is_none() {
                        return Ok(None);
                    }
                    let tuple_ref = TupleRef::new(bytes.unwrap(), &self.table.schema);
                    if let Some(predicate) = self.filter {
                        match predicate.evaluate_ref(Some(&tuple_ref))? {
                            ValueRef::Boolean(false) | ValueRef::Null => {}
                            ValueRef::Boolean(true) => {
                                self.current_slot += 1;

                                // Wildcard
                                if self.cols.is_empty() {
                                    return Ok(Some(tuple_ref.to_owned()?));
                                }

                                return Ok(Some(match_output_schema(&tuple_ref)?));
                            }
                            other => {
                                return Err(InvalidComparison(format!(
                                    "cannot valuate {:?}",
                                    other
                                )));
                            }
                        }
                    } else {
                        self.current_slot += 1;

                        // Wildcard
                        if self.cols.is_empty() {
                            return Ok(Some(tuple_ref.to_owned()?));
                        }
                        return Ok(Some(match_output_schema(&tuple_ref)?));
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
