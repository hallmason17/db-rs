/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */

use crate::{database::Database, error::Result, tables::TableSchema};

pub struct CreateTableExecutor<'a> {
    db: &'a mut Database,
    name: &'a str,
    schema: &'a TableSchema,
}

impl<'a> CreateTableExecutor<'a> {
    pub fn new(db: &'a mut Database, name: &'a str, schema: &'a TableSchema) -> Self {
        Self { db, name, schema }
    }

    pub fn execute(&mut self) -> Result<()> {
        let _ = self.db.create_table(self.name, self.schema)?;
        Ok(())
    }
}
