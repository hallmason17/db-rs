use std::{
    io::Read,
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
    planner::{binder::Binder, planner::Planner},
    storage::StorageManager,
    transaction::Transaction,
};

#[allow(dead_code)]
pub struct DbWorker {
    job_queue: mpsc::Receiver<String>,
    db: Database,
}
impl DbWorker {
    pub fn new(job_queue: mpsc::Receiver<String>) -> Result<Self, Box<dyn std::error::Error>> {
        let path = std::env::current_dir()?;
        Ok(Self {
            job_queue,
            db: Database::open(
                path.clone(),
                BufferPool::new(
                    1024 * 16,
                    ReplacementStrategy::Clock,
                    StorageManager::new(&path)?,
                )?,
            )?,
        })
    }

    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let planner = Planner::new();
        let mut binder = Binder::new();
        loop {
            let job = self.job_queue.recv()?;
            let start = Instant::now();
            tracing::info!("Got job: {job:#?}");
            let ast = Parser::parse_sql(&sqlparser::dialect::GenericDialect {}, &job)?;
            let statement = binder.bind(ast.first().unwrap().clone(), &self.db)?;
            let plan = planner.plan(statement);
            let txn = Transaction::new(&mut self.db);
            let executor = Executor::new(&txn);
            let rows = executor.execute(plan)?;
            println!("Returned {} rows in {:?}", rows.len(), start.elapsed());
        }
    }
}

#[allow(dead_code)]
pub struct Server {
    job_queue: mpsc::Sender<String>,
    worker_thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let (sender, receiver) = mpsc::channel();
        let worker_thread = Some(std::thread::spawn(move || {
            let mut dbworker = DbWorker::new(receiver).unwrap();
            dbworker.run().unwrap()
        }));
        Ok(Self {
            job_queue: sender,
            worker_thread,
        })
    }
    fn handle_conn(
        mut stream: TcpStream,
        job_queue: mpsc::Sender<String>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("handle_conn()");
        let mut inputbuf = [0u8; 4096];
        let bytes = stream.read(&mut inputbuf)?;
        job_queue.send(
            String::from_utf8_lossy(&inputbuf[..bytes])
                .trim()
                .to_string(),
        )?;
        tracing::info!("{job_queue:#?}");
        Ok(())
    }
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind("127.0.0.1:6767")?;
        tracing::info!("Listening on port: 6767");
        loop {
            let (stream, _) = listener.accept()?;
            tracing::info!("{stream:#?}");
            let sender = self.job_queue.clone();
            std::thread::spawn(move || {
                Self::handle_conn(stream, sender).unwrap();
            });
        }
    }
}
