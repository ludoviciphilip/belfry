//! belfryd: the Belfry background service.
//!
//!   belfryd run [--data-dir D] [--engine-dir E] [--listen ADDR] [--owner-uid N]
//!   belfryd install [--from DIR] [--owner-uid N]     (admin/root)
//!   belfryd uninstall [--purge]                      (admin/root)
//!   belfryd service                                  (Windows service entry)
//!   belfryd probe                                    (what friends would dial)
//!   belfryd version

mod api;
mod chat;
mod daemon;
mod engine;
mod install;
mod platform;
mod stun;
#[cfg(windows)]
mod winsvc;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub struct Opts {
    pub data_dir: PathBuf,
    pub engine_dir: Option<PathBuf>,
    pub listen: String,
    pub owner_uid: Option<u32>,
    pub from: Option<PathBuf>,
    pub purge: bool,
}

fn parse(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        data_dir: belfry_proto::default_data_dir(),
        engine_dir: None,
        listen: std::env::var("BELFRY_LISTEN").unwrap_or_else(|_| belfry_proto::DAEMON_ADDR.into()),
        owner_uid: None,
        from: None,
        purge: false,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().cloned().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--data-dir" => o.data_dir = val()?.into(),
            "--engine-dir" => o.engine_dir = Some(val()?.into()),
            "--listen" => o.listen = val()?,
            "--owner-uid" => o.owner_uid = Some(val()?.parse().map_err(|_| "bad uid")?),
            "--from" => o.from = Some(val()?.into()),
            "--purge" => o.purge = true,
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(o)
}

/// Runs until `stop` is set. Used by the foreground mode and the service.
pub fn run_daemon(o: &Opts, stop: Arc<AtomicBool>) -> Result<(), String> {
    let engine = engine::Engine::locate(o.engine_dir.clone())?;
    std::fs::create_dir_all(&o.data_dir).map_err(|e| format!("{}: {e}", o.data_dir.display()))?;
    let d = daemon::Daemon::new(o.data_dir.clone(), engine);
    let token = api::load_or_create_token(&d, o.owner_uid).map_err(|e| e.to_string())?;
    eprintln!(
        "belfryd {} | engine {} | {} | data {}",
        belfry_proto::VERSION,
        d.status().engine_version,
        o.listen,
        o.data_dir.display()
    );
    {
        let d = d.clone();
        std::thread::spawn(move || loop {
            d.refresh_public();
            std::thread::sleep(Duration::from_secs(300));
        });
    }
    {
        let (d, listen) = (d.clone(), o.listen.clone());
        std::thread::spawn(move || {
            if let Err(e) = api::serve(d, &listen, token) {
                eprintln!("API failed on {listen}: {e}");
                std::process::exit(2);
            }
        });
    }
    {
        let d = d.clone();
        let port = std::env::var("BELFRY_PEER_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(belfry_proto::PEER_PORT);
        std::thread::spawn(move || {
            if let Err(e) = api::serve_peers(d, port) {
                // Chat is optional: rooms keep working without it.
                eprintln!("chat listener failed on port {port}: {e}");
            }
        });
    }
    {
        let d = d.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(15));
            d.sync_chats();
        });
    }
    while !stop.load(Ordering::Relaxed) {
        d.supervise();
        std::thread::sleep(Duration::from_secs(1));
    }
    d.stop_all();
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => ("help", &[][..]),
    };
    let opts = match parse(rest) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("belfryd: {e}");
            std::process::exit(2);
        }
    };
    let result = match cmd {
        "run" => {
            let stop = Arc::new(AtomicBool::new(false));
            #[cfg(unix)]
            unix_signals(stop.clone());
            run_daemon(&opts, stop)
        }
        "install" => install::install(&opts),
        "uninstall" => install::uninstall(&opts),
        #[cfg(windows)]
        "service" => winsvc::run(),
        "probe" => {
            println!("public IPv4: {}", stun::public_v4().map(|i| i.to_string()).unwrap_or("not found".into()));
            println!("public IPv6: {}", stun::public_v6().map(|i| i.to_string()).unwrap_or("not found".into()));
            Ok(())
        }
        "version" | "--version" | "-V" => {
            println!("belfryd {}", belfry_proto::VERSION);
            Ok(())
        }
        _ => {
            eprintln!("usage: belfryd run|install|uninstall|probe|version  (see README)");
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("belfryd: {e}");
        std::process::exit(1);
    }
}

#[cfg(unix)]
fn unix_signals(stop: Arc<AtomicBool>) {
    static FLAG: AtomicBool = AtomicBool::new(false);
    extern "C" fn on_signal(_: libc::c_int) {
        FLAG.store(true, Ordering::Relaxed);
    }
    let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
    unsafe {
        libc::signal(libc::SIGTERM, handler);
        libc::signal(libc::SIGINT, handler);
    }
    std::thread::spawn(move || loop {
        if FLAG.load(Ordering::Relaxed) {
            stop.store(true, Ordering::Relaxed);
            return;
        }
        std::thread::sleep(Duration::from_millis(200));
    });
}
