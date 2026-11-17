//! A local probe: serve the phone's command bus against an arbitrary data directory.
//!
//! The fork used to add these flags to `headless/src/main.rs` (`--command-bus-port`,
//! `--command-bus-only`), which meant 66 lines inside a crate that also exists upstream. The probe is
//! a developer tool for this fork, not a headless-host feature, so it lives here instead - it reuses
//! the same host builder and the same command registry the phone uses, so a `curl` against it exercises
//! exactly the surface the app talks to.
//!
//! ```text
//! cargo run -p vrcx-0-mobile-core --bin vrcx-0-mobile-probe -- \
//!     --data-dir <dir> --command-bus-port 8100
//! ```
//!
//! The data directory is used as `--data-dir` on the desktop would be: the host keeps its own
//! `VRCX-0` folder inside it, so pointing at a copy of a desktop profile reproduces the phone's view
//! of that profile.

use std::path::PathBuf;
use std::process::ExitCode;

fn arg_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(data_dir) = arg_value(&args, "--data-dir") else {
        eprintln!("usage: vrcx-0-mobile-probe --data-dir <dir> [--command-bus-port <port>]");
        return ExitCode::from(2);
    };
    let port: u16 = arg_value(&args, "--command-bus-port")
        .and_then(|value| value.parse().ok())
        .unwrap_or(8100);

    let state = match vrcx_0_mobile_core::host::build_host(&PathBuf::from(data_dir), "probe") {
        Ok(state) => std::sync::Arc::new(state),
        Err(error) => {
            eprintln!("probe: cannot build host: {error}");
            return ExitCode::from(1);
        }
    };

    let bus = vrcx_0_mobile_host::build_bus(std::sync::Arc::clone(&state));
    match vrcx_0_command_server::serve_on(bus, port).await {
        Ok((address, handle)) => {
            // Kept alive for the lifetime of the process.
            std::mem::forget(handle);
            println!("command bus listening on http://{address}/api/invoke");
            println!("POST {{\"cmd\": \"sync__status\", \"args\": {{}}}}");
        }
        Err(error) => {
            eprintln!("probe: cannot bind port {port}: {error}");
            return ExitCode::from(1);
        }
    }

    let _ = tokio::signal::ctrl_c().await;
    ExitCode::SUCCESS
}
