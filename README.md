# db-rs
db-rs is an educational database engine written in Rust.

The project is under active development.

### Features/Status
- [x] Heap page storage (slotted pages)
- [x] Buffer pool page cache
- [x] Multi-table support
- [x] Server binary with TCP listener
- [x] Sequential scans with filters
- [ ] Indexing (Btree)
- [ ] BNL join
- [ ] Hash join
- [ ] Merge join
- [ ] Row pretty printer/formatter

### SQL
- [x] Basic SELECT
- [x] Basic WHERE
- [ ] CREATE TABLE
- [ ] INSERT
- [ ] UPDATE
- [ ] DELETE
- [ ] DROP
- [ ] TRUNCATE
- [ ] EXPLAIN
- [ ] ANALYZE
- [ ] LIMIT


## Usage
```bash
cargo build
cargo test
cargo run --bin db # tcp server
cargo run --bin db-rs # small demo
cargo bench
```
