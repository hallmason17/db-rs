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
    execution::executor::Executor,
    planner::{binder::Binder, plan::Planner},
    storage::StorageManager,
    tables::Tuple,
    transaction::Transaction,
};

#[derive(Debug)]
pub struct Job {
    sql_string: String,
    sender: mpsc::Sender<Vec<Tuple>>,
}

#[allow(dead_code)]
pub struct DbWorker {
    job_queue: mpsc::Receiver<Job>,
    db: Database,
}
impl DbWorker {
    pub fn new(job_queue: mpsc::Receiver<Job>) -> Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::current_dir()?;
        Ok(Self {
            job_queue,
            db: Database::open(
                path.clone(),
                BufferPool::new(128, ReplacementStrategy::Clock, StorageManager::new(&path)?)?,
            )?,
        })
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let planner = Planner::new();
        let mut binder = Binder::new();
        loop {
            tracing::info!("Waiting for jobs!");
            let job = self.job_queue.recv()?;
            let start = Instant::now();
            let ast = Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, &job.sql_string)?;
            if let Some(ast1) = ast.first() {
                let statement = binder.bind(ast1.clone(), &self.db)?;
                let plan = planner.plan(statement);
                let mut txn = Transaction::new(&mut self.db);
                let mut executor = Executor::new(&mut txn);
                let rows = executor.execute(plan)?;
                let len = rows.len();
                let fin = start.elapsed();
                job.sender.send(rows)?;
                println!("Returned {} rows in {:?}", len, fin);
            }
        }
    }
}

#[allow(dead_code)]
pub struct Server {
    job_queue: mpsc::Sender<Job>,
    worker_thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (sender, receiver) = mpsc::channel();
        let worker_thread = Some(std::thread::spawn(move || {
            let mut dbworker = DbWorker::new(receiver).unwrap();
            loop {
                let res = dbworker.run();
                if res.is_err() {
                    tracing::error!("{:?}", res);
                }
            }
        }));
        Ok(Self {
            job_queue: sender,
            worker_thread,
        })
    }
    fn handle_conn(
        mut stream: TcpStream,
        job_queue: mpsc::Sender<Job>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tracing::debug!("handleconn()");
        let mut inputbuf = [0u8; 4096];
        let bytes = stream.read(&mut inputbuf)?;
        let (sender, receiver) = mpsc::channel();
        let job = Job {
            sql_string: String::from_utf8_lossy(&inputbuf[..bytes])
                .trim()
                .to_string(),
            sender,
        };
        job_queue.send(job)?;
        while let Ok(row) = receiver.recv() {
            let _ = stream.write(format!("{:?}\r\n", row).as_bytes())?;
        }
        Ok(())
    }
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:6767")?;
        tracing::info!("Listening on port: 6767");
        loop {
            let (stream, _) = listener.accept()?;
            let sender = self.job_queue.clone();
            std::thread::spawn(move || {
                Self::handle_conn(stream, sender).unwrap();
            });
        }
    }
}
