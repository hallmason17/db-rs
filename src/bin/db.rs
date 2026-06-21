/* Copyright (C) 2026 Mason Hall.
 *
 * GNU General Public License v3.0+ (see COPYING or https://www.gnu.org/licenses/gpl-3.0.txt)
 */
use db_rs::server::Server;
use tracing_subscriber::{EnvFilter, Layer, fmt, layer::SubscriberExt, util::SubscriberInitExt};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logger();
    let mut server = Server::new()?;
    server.run()
}

fn setup_logger() {
    let fmt_layer = fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_target(false)
        .with_filter(EnvFilter::from_default_env());
    tracing_subscriber::registry().with(fmt_layer).init();
}
