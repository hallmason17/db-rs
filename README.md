# db-rs
db-rs is an educational database engine written in Rust.

The project is under active development.

### Currently implemented:
- tuple insertion
- page-file storage
- slotted pages
- buffer pool
- catalog metadata
- tuple serialization
- free-space tracking
- naive insert benchmarks.

### Planned:
- Table scans
- SQL query support
- WAL

## Usage
```bash
cargo build
cargo test
cargo run
cargo bench
```

