# db-rs
db-rs is an educational database engine written in Rust.

The project is under active development.

## Basic Usage

### The Demo
The project includes a small demo showing some of the basic features. You can run it with `cargo r --bin demo`. It initializes a database in a temp directory and creates a table, inserts records into it, and runs a basic `SELECT` query and returns output like the following:

``` bash

Insert + seq scan demo.
		Run with RUST_LOG=log_level to see logs.
Creating db at TempDir { path: "/tmp/db-rs.RCuXGwvSPz6Y" }
CREATE TABLE users (id INT PRIMARY KEY, name VARCHAR(50), email VARCHAR(50) NOT NULL);
INSERT INTO users (id, name, email) VALUES (0,'mason0','mason0@example.com');
INSERT INTO users (id, name, email) VALUES (1,'mason1','mason1@example.com');
INSERT INTO users (id, name, email) VALUES (2,'mason2','mason2@example.com');
INSERT INTO users (id, name, email) VALUES (3,'mason3','mason3@example.com');
...
INSERT INTO users (id, name, email) VALUES (997,'mason997','mason997@example.com');
INSERT INTO users (id, name, email) VALUES (998,'mason998','mason998@example.com');
INSERT INTO users (id, name, email) VALUES (999,'mason999','mason999@example.com');
Inserted 1000 rows in 25.485745ms
select id, name from users where (id < 1000) and (name < 'mason5');
Returned 445 rows in 996.503µs
```

### TCP Server
There is also a binary included to run the database behind a TCP server (similar to many other databases). While the database can handle multiple clients simultaneously, the engine is single-threaded. As such, query requests are handled sequentially.


There is no included client application at the moment. In the meantime, I've been interacting with it via `nc` like so (I currently just return the Rust "Debug" output for tuples):

``` bash
~/repos/db-rs echo "create table demo1 (id int, col1 varchar not null, col2 float);" | nc localhost 6767
[]
~/repos/db-rs echo "insert into demo1 (id, col1, col2) values(1,'asdf', 1.0);" | nc localhost 6767
[]
~/repos/db-rs echo "select * from demo1;" | nc localhost 6767
[Tuple { values: [Int(1), VarChar("asdf"), Float(1.0)] }]
```

## Features/Status
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

## SQL
- [x] Basic SELECT
- [x] Basic WHERE
- [x] CREATE TABLE
- [x] INSERT
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
cargo run --bin demo # small demo
cargo run --bin db # tcp server
cargo bench
```

## License

This project is licensed under the GNU General Public License v3.0 (or later).
See the [COPYING](COPYING) file for details.
