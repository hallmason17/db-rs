use db_rs::server::Server;
use tracing_subscriber::{Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logger();
    let mut server = Server::new()?;
    server.run()
}

fn setup_logger() {
    /*
        let fmt_layer = fmt::layer()
            .with_file(true)
            .with_line_number(true)
            .with_target(false);
    */
    //tracing_subscriber::registry().with(fmt_layer).init();
}
