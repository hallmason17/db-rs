use crate::{
    buffer_pool::BufferPool,
    database::Database,
    error::{Error, InputError, Result},
    ids::{PageId, TableId},
    page::{PAGE_SIZE, PageKind, SlotArrayEntry, SlottedPageMut, page_header_offsets},
    tables::{RecordId, Table},
};

pub struct InsertExecutor<'a> {
    db: &'a mut Database,
    table_id: TableId,
    record: Vec<u8>,
}

impl<'a> InsertExecutor<'a> {
    pub fn new(db: &'a mut Database, table_id: TableId, record: Vec<u8>) -> Self {
        Self {
            db,
            table_id,
            record,
        }
    }

    pub fn execute(&mut self) -> Result<RecordId> {
        let table = self.db.tables.get_mut(&self.table_id).unwrap();
        let bp = &mut self.db.buffer_manager;
        insert_record(table, bp, &self.record)
    }
}

fn try_insert_into_page(
    table: &mut Table,
    bp: &mut BufferPool,
    page_num: u64,
    record: &[u8],
) -> Result<RecordId> {
    let mut page = if let Ok(page) = bp.get_page(PageId {
        file_id: table.file_id,
        page_num,
    }) {
        page
    } else {
        bp.create_page(table.file_id, PageKind::Heap)?
    };
    table.current_heap_page = page.page_id.page_num;

    let rid = page.with_heap_mut(|heap| match heap.insert(record) {
        Ok(slot) => Ok(RecordId {
            page: slot.page,
            slot: slot.slot,
        }),
        Err(e) => Err(e),
    })?;

    Ok(rid)
}

fn handle_full_page(table: &mut Table, bp: &mut BufferPool) -> Result<()> {
    let guard = bp.create_page(table.file_id, PageKind::Heap)?;
    table.current_heap_page = guard.page_id.page_num;
    Ok(())
}

fn insert_record(table: &mut Table, bp: &mut BufferPool, record: &[u8]) -> Result<RecordId> {
    if record.len() > PAGE_SIZE - page_header_offsets::SIZE - size_of::<SlotArrayEntry>() {
        return Err(Error::InputError(InputError::RecordTooLarge));
    }

    loop {
        match try_insert_into_page(table, bp, table.current_heap_page, record) {
            Ok(rid) => return Ok(rid),
            Err(Error::PageFull) => handle_full_page(table, bp)?,
            Err(e) => return Err(e),
        }
    }
}
