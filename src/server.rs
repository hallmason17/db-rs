/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread::JoinHandle,
    time::Instant,
};

use sqlparser::parser::Parser;

use crate::{
    buffer_pool::{BufferPool, ReplacementStrategy},
    database::Database,
    error::{Error, Result},
    execution::executor::Executor,
    planner::{binder::Binder, plan::Planner},
    storage::StorageManager,
    tables::Tuple,
    transaction::Transaction,
};

#[derive(Debug)]
pub struct Job {
    sql_string: String,
    sender: mpsc::Sender<Result<Vec<Tuple>>>,
}

#[allow(dead_code)]
pub struct DbWorker {
    job_queue: mpsc::Receiver<Job>,
    db: Database,
}
impl DbWorker {
    pub fn new(job_queue: mpsc::Receiver<Job>) -> Result<Self> {
        let path = std::env::current_dir()?;
        Ok(Self {
            job_queue,
            db: Database::open(
                path.clone(),
                BufferPool::new(128, ReplacementStrategy::Clock, StorageManager::new(&path)?)?,
            )?,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        loop {
            tracing::info!("Waiting for jobs!");
            let job = match self.job_queue.recv() {
                Ok(job) => job,
                Err(_) => {
                    tracing::info!("job queue closed, worker shutting down");
                    return Ok(());
                }
            };
            let start = Instant::now();
            let statements = job.sql_string.split(";");
            for statement in statements {
                if statement.is_empty() {
                    continue;
                }
                let result = execute_sql(&mut self.db, statement);
                match &result {
                    Ok(rows) => {
                        println!("Returned {} rows in {:?}", rows.len(), start.elapsed());
                    }
                    Err(e) => {
                        tracing::error!("query failed: {e}");
                    }
                }
                if let Err(e) = job.sender.send(result) {
                    tracing::error!("failed to send query result to client: {e}");
                }
            }
        }
    }
}

fn execute_sql(db: &mut Database, sql: &str) -> Result<Vec<Tuple>> {
    let planner = Planner::new();
    let mut binder = Binder::new();
    let ast = Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, sql)
        .map_err(|e| Error::ParseError(e.to_string()))?;
    tracing::debug!("Parsed SQL: {:?}", ast);
    let Some(statement) = ast.into_iter().next() else {
        return Ok(Vec::new());
    };
    let statement = binder.bind(statement, db)?;
    tracing::debug!("Bound statement: {:?}", statement);
    let plan = planner.plan(statement);
    tracing::debug!("Generated plan: {:?}", plan);
    let mut txn = Transaction::new(db);
    let mut executor = Executor::new(&mut txn);
    executor.execute(plan)
}

#[allow(dead_code)]
pub struct Server {
    job_queue: mpsc::Sender<Job>,
    worker_thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new() -> Result<Self> {
        let (sender, receiver) = mpsc::channel();
        let worker_thread = Some(std::thread::spawn(move || {
            let mut dbworker = match DbWorker::new(receiver) {
                Ok(worker) => worker,
                Err(e) => {
                    tracing::error!("failed to start database worker: {e}");
                    return;
                }
            };
            tracing::info!("Worker thread started");
            if let Err(e) = dbworker.run() {
                tracing::error!("worker stopped: {e}");
            }
        }));
        Ok(Self {
            job_queue: sender,
            worker_thread,
        })
    }
    fn handle_conn(mut stream: TcpStream, job_queue: mpsc::Sender<Job>) -> Result<()> {
        tracing::debug!("handleconn() start");
        let mut inputbuf = [0u8; 4096];
        let bytes = stream.read(&mut inputbuf)?;
        let (sender, receiver) = mpsc::channel();
        let job = Job {
            sql_string: String::from_utf8_lossy(&inputbuf[..bytes])
                .trim()
                .to_string(),
            sender,
        };
        job_queue.send(job).map_err(|_| Error::Unknown)?;
        for message in receiver {
            let reply = match message {
                Ok(rows) => format!("{rows:?}\r\n"),
                Err(e) => format!("ERROR: {e}\r\n"),
            };
            stream.write_all(reply.as_bytes())?;
        }
        tracing::debug!("handleconn() finished");
        Ok(())
    }
    pub fn run(&mut self) -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:6767")?;
        tracing::info!("Listening on port: 6767");
        loop {
            let (stream, addr) = listener.accept()?;
            tracing::info!("Accepted connection from {}", addr);
            let sender = self.job_queue.clone();
            std::thread::spawn(move || {
                if let Err(e) = Self::handle_conn(stream, sender) {
                    tracing::error!("connection error: {e}");
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        tables::{ColumnDefinition, TableSchema},
        value::DataType,
    };
    use tempfile::tempdir;

    fn setup_db() -> (tempfile::TempDir, Database) {
        let dir = tempdir().unwrap();
        let sm = StorageManager::new(dir.path()).unwrap();
        let bp = BufferPool::new(16, ReplacementStrategy::Clock, sm).unwrap();
        let mut db = Database::open(dir.path().into(), bp).unwrap();
        let schema = TableSchema::new(&[
            ColumnDefinition::new("id".into(), DataType::Int, false).unwrap(),
            ColumnDefinition::new("name".into(), DataType::VarChar, true).unwrap(),
        ]);
        db.create_table("users", &schema).unwrap();
        (dir, db)
    }

    #[test]
    fn query_error_does_not_prevent_later_queries() {
        let (_dir, mut db) = setup_db();

        let err = execute_sql(&mut db, "select id from missing").unwrap_err();
        assert!(matches!(err, Error::TableNotFound(table) if table == "missing"));

        let parse_err = execute_sql(&mut db, "select asdf").unwrap_err();
        assert!(matches!(parse_err, Error::ParseError(_)));

        let rows = execute_sql(&mut db, "select id from users").unwrap();
        assert!(rows.is_empty());
    }
}
