use std::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread::JoinHandle,
    time::Duration,
};

use crate::database::Database;

pub struct DbWorker {
    job_queue: mpsc::Receiver<i64>,
    db: Option<Database>,
}
impl DbWorker {
    pub fn new(job_queue: mpsc::Receiver<i64>) -> Self {
        Self {
            job_queue,
            db: None,
        }
    }

    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            let job = self.job_queue.recv()?;
            tracing::info!("Got job: {job:#?}");
            tracing::info!("{:?}", self.db);
            std::thread::sleep(Duration::from_millis(100));
        }
    }
}

pub struct Server {
    job_queue: mpsc::Sender<i64>,
    worker_thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        let worker_thread = Some(std::thread::spawn(move || {
            let dbworker = DbWorker::new(receiver);
            dbworker.run().unwrap()
        }));
        Self {
            job_queue: sender,
            worker_thread,
        }
    }
    fn handle_conn(
        _stream: TcpStream,
        job_queue: mpsc::Sender<i64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("handle_conn()");
        job_queue.send(1)?;
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

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}
