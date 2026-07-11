use anyhow::Error;

pub mod login;
pub mod settings;
pub mod whoami;

// Force rockbox-airplay and rockbox-slim symbols into librockbox_cli.a
use clap::Command;
use owo_colors::OwoColorize;
#[allow(unused_imports)]
use rockbox_airplay::_link_airplay as _;
#[cfg(feature = "alsa-sink")]
#[allow(unused_imports)]
use rockbox_alsa_sink::_link_alsa_sink as _;
#[allow(unused_imports)]
use rockbox_chromecast::_link_chromecast as _;
#[allow(unused_imports)]
use rockbox_cmaf::_link_cmaf as _;
#[cfg(feature = "cpal-sink")]
#[allow(unused_imports)]
use rockbox_cpal_sink::_link_cpal_sink as _;
#[allow(unused_imports)]
use rockbox_hls::_link_hls as _;
use rockbox_library::audio_scan::{save_audio_metadata, scan_audio_files};
use rockbox_library::{create_connection_pool, repo};
#[cfg(not(feature = "fts5"))]
use rockbox_playlists::PlaylistStore;
#[allow(unused_imports)]
use rockbox_slim::_link_slim as _;
#[cfg(feature = "sndio-sink")]
#[allow(unused_imports)]
use rockbox_sndio_sink::_link_sndio_sink as _;
#[cfg(not(feature = "fts5"))]
use rockbox_typesense::client::*;
#[cfg(not(feature = "fts5"))]
use rockbox_typesense::types::*;
#[allow(unused_imports)]
use rockbox_upnp::_link_upnp as _;
#[cfg(not(feature = "fts5"))]
use std::io::{BufRead, BufReader};
#[cfg(not(feature = "fts5"))]
use std::process::Stdio;
#[cfg(not(feature = "fts5"))]
use std::sync::atomic::{AtomicI32, Ordering};
use std::thread::sleep;
use std::time::Duration;
use std::{env, ffi::CStr};
use std::{fs, thread};
#[cfg(not(feature = "fts5"))]
use tracing::error;
use tracing::{info, warn};

/// PID of the spawned typesense-server child, or -1 if not yet started.
#[cfg(not(feature = "fts5"))]
static TYPESENSE_PID: AtomicI32 = AtomicI32::new(-1);

/// Poll the Typesense health endpoint until it responds, giving the server
/// time to start before any collection or indexing calls are made.
#[cfg(not(feature = "fts5"))]
async fn wait_for_typesense() {
    let port = std::env::var("RB_TYPESENSE_PORT").unwrap_or_else(|_| "8109".to_string());
    let url = format!("http://localhost:{}/health", port);
    let client = reqwest::Client::new();
    info!(
        "Waiting for Typesense to accept connections on port {}...",
        port
    );
    for attempt in 1..=30 {
        match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => {
                info!("Typesense is ready (took {} attempt(s))", attempt);
                return;
            }
            Ok(r) => {
                tracing::debug!(
                    "Typesense health check attempt {}: HTTP {}",
                    attempt,
                    r.status()
                );
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => {
                tracing::debug!("Typesense health check attempt {}: {}", attempt, e);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
    warn!("Typesense did not become ready within 30s; proceeding anyway");
}

/// SIGTERM/SIGINT handler: kill the typesense child then _exit immediately.
///
/// system-hosted.c installs a SIGTERM handler that calls system_exception_wait()
/// which loops forever waiting for an SDL quit event that never arrives on a
/// headless daemon.  We override it here with a handler that actually exits.
/// _exit is used because it is async-signal-safe (exit() is not).
#[cfg(unix)]
extern "C" fn handle_shutdown(_sig: libc::c_int) {
    #[cfg(not(feature = "fts5"))]
    {
        let pid = TYPESENSE_PID.load(Ordering::SeqCst);
        if pid > 0 {
            unsafe { libc::kill(pid, libc::SIGTERM) };
        }
    }
    unsafe { libc::_exit(0) };
}

#[cfg(unix)]
fn raise_fd_limit() {
    unsafe {
        let mut rlim: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut rlim) == 0 {
            // Raise to 4096 or the hard limit, whichever is lower.
            // Cast through libc::rlim_t so this compiles on both 32-bit ARM
            // (rlim_t = u32) and 64-bit hosts (rlim_t = u64).
            let target = (4096 as libc::rlim_t).min(rlim.rlim_max);
            if rlim.rlim_cur < target {
                rlim.rlim_cur = target;
                libc::setrlimit(libc::RLIMIT_NOFILE, &rlim);
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn parse_args(argc: usize, argv: *const *const u8) -> i32 {
    #[cfg(unix)]
    raise_fd_limit();

    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    let string_array = unsafe { std::slice::from_raw_parts(argv, argc) };
    let args: Vec<&str> = string_array
        .iter()
        .map(|&ptr| {
            let c_str = unsafe { CStr::from_ptr(ptr as *const std::ffi::c_char) };
            c_str
                .to_str()
                .unwrap_or("[Invalid UTF-8 or Non Null-Terminated String]")
        })
        .collect();

    const VERSION: &str = match option_env!("TAG") {
        Some(tag) => tag,
        None => env!("CARGO_PKG_VERSION"),
    };

    let banner = format!(
        "{}\nA fork of the original Rockbox project, with a focus on modernization and more features.",
        r#"
              __________               __   ___.
    Open      \______   \ ____   ____ |  | _\_ |__   _______  ___
    Source     |       _//  _ \_/ ___\|  |/ /| __ \ /  _ \  \/  /
    Jukebox    |    |   (  <_> )  \___|    < | \_\ (  <_> > <  <
    Firmware   |____|_  /\____/ \___  >__|_ \|___  /\____/__/\_ \
                      \/            \/     \/    \/            \/
    "#
        .yellow()
    );
    let cli = Command::new("rockboxd")
        .version(VERSION)
        .about(&banner)
        .subcommand(
            Command::new("settings")
                .about("Manage Rockbox audio settings synced with Rocksky")
                .subcommand_required(true)
                .arg_required_else_help(true)
                .subcommand(
                    Command::new("pull")
                        .about("Pull audio settings from Rocksky and apply them")
                        .arg(
                            clap::Arg::new("did")
                                .long("did")
                                .value_name("DID_OR_HANDLE")
                                .help("Fetch another user's settings publicly (no login required)")
                                .required(false),
                        ),
                )
                .subcommand(Command::new("push").about("Push local audio settings to Rocksky")),
        )
        .subcommand(
            Command::new("login")
                .about("Login to your Rocksky account")
                .arg(
                    clap::Arg::new("handle")
                        .required(true)
                        .help("Your Bluesky handle"),
                ),
        )
        .subcommand(Command::new("whoami").about("Show the currently logged-in Rocksky user"));

    let matches = cli.get_matches_from(args);

    // Dispatch CLI-only subcommands and exit the process — do NOT start the server.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    match matches.subcommand() {
        Some(("settings", sub_m)) => {
            let result = match sub_m.subcommand() {
                Some(("pull", m)) => {
                    let did = m.get_one::<String>("did").cloned();
                    rt.block_on(settings::pull(did))
                }
                Some(("push", _)) => rt.block_on(settings::push()),
                _ => unreachable!(),
            };
            match result {
                Ok(_) => std::process::exit(0),
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(("login", sub_m)) => {
            let handle = sub_m.get_one::<String>("handle").unwrap();
            match rt.block_on(login::login(handle)) {
                Ok(_) => std::process::exit(0),
                Err(e) => {
                    eprintln!("Error: {e}");
                    std::process::exit(1);
                }
            }
        }
        Some(("whoami", _)) => match rt.block_on(whoami::whoami()) {
            Ok(_) => std::process::exit(0),
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        _ => {} // Fall through to starting the Rockbox server
    }

    // Install shutdown handler before spawning typesense-server so the PID is
    // always available when the handler fires.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGTERM, handle_shutdown as libc::sighandler_t);
        libc::signal(libc::SIGINT, handle_shutdown as libc::sighandler_t);
    }

    // SDL (initialised after parse_args returns) installs its own SIGTERM/SIGINT
    // handlers and overwrites ours.  Reinstall after a short delay so our
    // handler — which kills typesense-server and _exit()s — wins.
    #[cfg(unix)]
    thread::spawn(|| {
        sleep(Duration::from_secs(3));
        unsafe {
            libc::signal(libc::SIGTERM, handle_shutdown as libc::sighandler_t);
            libc::signal(libc::SIGINT, handle_shutdown as libc::sighandler_t);
        }
    });

    thread::spawn(move || {
        let home = env::var("HOME").unwrap();

        match fs::create_dir_all(format!("{}/Music", home)) {
            Ok(_) => {}
            Err(e) => {
                tracing::error!("Failed to create Music directory: {}", e);
            }
        }

        let update_library = match env::var("ROCKBOX_UPDATE_LIBRARY")
            .as_ref()
            .map(|s| s.as_str())
        {
            Ok("1") => true,
            Ok("true") => true,
            Ok(_) => false,
            Err(_) => false,
        };
        let path = rockbox_settings::get_music_dir().unwrap_or(format!("{}/Music", home));
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async { run_indexing(path, update_library).await })
            .unwrap_or_else(|e| warn!("Library indexing failed: {}", e));

        thread::spawn(move || {
            sleep(Duration::from_secs(5));
            match rockbox_rocksky::register_rockbox() {
                Ok(_) => info!("Successfully registered Rockbox with Rocksky server"),
                Err(e) => tracing::debug!("Failed to register Rockbox with Rocksky server: {}", e),
            };
        });

        const BANNER: &str = r#"
          __________               __   ___.
Open      \______   \ ____   ____ |  | _\_ |__   _______  ___
Source     |       _//  _ \_/ ___\|  |/ /| __ \ /  _ \  \/  /
Jukebox    |    |   (  <_> )  \___|    < | \_\ (  <_> > <  <
Firmware   |____|_  /\____/ \___  >__|_ \|___  /\____/__/\_ \
                  \/            \/     \/    \/            \/
"#;

        println!("{}", BANNER.yellow());

        let port = std::env::var("ROCKBOX_TCP_PORT").unwrap_or_else(|_| "6063".to_string());
        let addr = format!("0.0.0.0:{}", port);

        info!("Rockbox TCP server is running on {}", addr);

        let graphql_port = env::var("ROCKBOX_GRAPHQL_PORT").unwrap_or("6062".to_string());
        let addr = format!("{}:{}", "0.0.0.0", graphql_port);

        info!("Rockbox GraphQL server is running on {}", addr);

        let rockbox_port: u16 = std::env::var("ROCKBOX_PORT")
            .unwrap_or_else(|_| "6061".to_string())
            .parse()
            .expect("ROCKBOX_PORT must be a number");

        let host_and_port = format!("0.0.0.0:{}", rockbox_port);

        info!("Rockbox gRPC server is running on {}", host_and_port);

        info!("Rockbox Web UI is running on http://localhost:6062");
    });

    spawn_typesense_subprocess();
    return 0;
}

#[cfg(not(feature = "fts5"))]
async fn run_indexing(path: String, update_library: bool) -> Result<(), Error> {
    info!("Setting up Typesense search engine...");
    rockbox_typesense::setup()?;

    // Wait for Typesense to accept connections before any HTTP calls.
    // The subprocess thread starts typesense-server concurrently, so it
    // may not be listening yet when we reach the first collection call.
    wait_for_typesense().await;

    info!("Connecting to library database...");
    let pool = create_connection_pool().await?;
    let tracks = repo::track::all(pool.clone()).await?;
    if tracks.is_empty() || update_library {
        if tracks.is_empty() {
            info!(
                "Library is empty — starting first-time audio scan of: {}",
                path
            );
        } else {
            info!(
                "ROCKBOX_UPDATE_LIBRARY set — rescanning audio library at: {}",
                path
            );
        }
        match scan_audio_files(pool.clone(), path.clone().into()).await {
            Ok(_) => info!("Audio scan complete"),
            Err(e) => error!("Failed to scan audio files: {}", e),
        }
        let tracks = repo::track::all(pool.clone()).await?;
        let albums = repo::album::all(pool.clone()).await?;
        let artists = repo::artist::all(pool.clone()).await?;

        info!(
            "Indexing {} tracks, {} albums, {} artists into Typesense...",
            tracks.len(),
            albums.len(),
            artists.len()
        );

        info!("Creating Typesense collections...");
        create_tracks_collection().await?;
        create_albums_collection().await?;
        create_artists_collection().await?;

        info!("Inserting {} tracks...", tracks.len());
        insert_tracks(tracks.into_iter().map(Track::from).collect()).await?;
        info!("Inserting {} artists...", artists.len());
        insert_artists(artists.into_iter().map(Artist::from).collect()).await?;
        info!("Inserting {} albums...", albums.len());
        insert_albums(albums.into_iter().map(Album::from).collect()).await?;

        info!("Search index build complete.");
    } else {
        info!(
            "Library already indexed ({} tracks); skipping scan.",
            tracks.len()
        );
    }

    info!("Setting up playlists collection...");
    create_playlists_collection().await?;
    let playlist_store = PlaylistStore::new(pool.clone());
    let saved = playlist_store.list().await.unwrap_or_default();
    let smart = playlist_store
        .list_smart_playlists()
        .await
        .unwrap_or_default();
    let ts_playlists: Vec<Playlist> = saved
        .into_iter()
        .map(|p| Playlist {
            id: p.id,
            name: p.name,
            description: p.description,
            image: p.image,
            is_smart: false,
            track_count: p.track_count,
        })
        .chain(smart.into_iter().map(|p| Playlist {
            id: p.id,
            name: p.name,
            description: p.description,
            image: p.image,
            is_smart: true,
            track_count: 0,
        }))
        .collect();
    if !ts_playlists.is_empty() {
        info!(
            "Indexing {} playlist(s) into Typesense...",
            ts_playlists.len()
        );
        insert_playlists(ts_playlists).await?;
        info!("Playlist index complete.");
    } else {
        info!("No playlists to index.");
    }

    if let Err(e) = rockbox_library::watcher::start_watcher(pool.clone(), path.into()) {
        warn!("Failed to start library watcher: {}", e);
    }
    Ok(())
}

#[cfg(feature = "fts5")]
async fn run_indexing(path: String, update_library: bool) -> Result<(), Error> {
    info!("Connecting to library database (FTS5 search backend)...");
    let pool = create_connection_pool().await?;
    let tracks = repo::track::all(pool.clone()).await?;
    if tracks.is_empty() || update_library {
        if tracks.is_empty() {
            info!(
                "Library is empty — starting first-time audio scan of: {}",
                path
            );
        } else {
            info!(
                "ROCKBOX_UPDATE_LIBRARY set — rescanning audio library at: {}",
                path
            );
        }
        match scan_audio_files(pool.clone(), path.clone().into()).await {
            Ok(_) => info!("Audio scan complete (FTS5 indexed via triggers)"),
            Err(e) => tracing::error!("Failed to scan audio files: {}", e),
        }
    } else {
        info!(
            "Library already indexed ({} tracks); FTS5 ready.",
            tracks.len()
        );
    }

    if let Err(e) = rockbox_library::watcher::start_watcher(pool.clone(), path.into()) {
        warn!("Failed to start library watcher: {}", e);
    }
    Ok(())
}

#[cfg(not(feature = "fts5"))]
fn spawn_typesense_subprocess() {
    thread::spawn(move || {
        let api_key = uuid::Uuid::new_v4().to_string();
        let api_key = std::env::var("RB_TYPESENSE_API_KEY").unwrap_or(api_key);
        std::env::set_var("RB_TYPESENSE_API_KEY", &api_key);
        info!("Using Typesense API key: {}", api_key);

        let port = std::env::var("RB_TYPESENSE_PORT").unwrap_or_else(|_| "8109".to_string());
        std::env::set_var("RB_TYPESENSE_PORT", &port);

        let homedir = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        let data_dir = homedir.join(".config/rockbox.org/typesense");

        let ts_bin = {
            let local = homedir.join(".rockbox/bin/typesense-server");
            if local.exists() {
                local
            } else {
                std::path::PathBuf::from("typesense-server")
            }
        };

        let mut cmd = std::process::Command::new(&ts_bin);
        cmd.arg("--enable-cors")
            .arg(format!("--api-port={port}"))
            .env("TYPESENSE_API_KEY", &api_key)
            .env("TYPESENSE_DATA_DIR", &data_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        #[cfg(target_os = "linux")]
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                libc::prctl(
                    libc::PR_SET_PDEATHSIG,
                    libc::SIGTERM as libc::c_ulong,
                    0,
                    0,
                    0,
                );
                Ok(())
            });
        }

        info!(
            "Starting typesense-server (binary: {}, port: {}, data: {})",
            ts_bin.display(),
            port,
            data_dir.display()
        );
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                error!(
                    "Failed to spawn typesense-server ({}): {}",
                    ts_bin.display(),
                    e
                );
                return Err(e.into());
            }
        };
        let pid = child.id();
        TYPESENSE_PID.store(pid as i32, Ordering::SeqCst);
        info!("typesense-server started with PID {}", pid);

        if let Some(stdout) = child.stdout.take() {
            thread::spawn(move || {
                for line in BufReader::new(stdout).lines().flatten() {
                    tracing::debug!(target: "typesense", "{}", line);
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            thread::spawn(move || {
                for line in BufReader::new(stderr).lines().flatten() {
                    tracing::warn!(target: "typesense", "{}", line);
                }
            });
        }

        // Poll instead of blocking in waitpid so SIGTERM can reach the process.
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    warn!("typesense-server exited: {status}");
                    break;
                }
                Ok(None) => sleep(Duration::from_millis(500)),
                Err(e) => {
                    error!("typesense-server monitor error: {e}");
                    break;
                }
            }
        }

        Ok::<(), Error>(())
    });
}

#[cfg(feature = "fts5")]
fn spawn_typesense_subprocess() {
    info!("FTS5 search backend enabled — skipping typesense-server spawn.");
}

#[no_mangle]
pub extern "C" fn save_remote_track_metadata(url: *const std::ffi::c_char) -> i32 {
    if url.is_null() {
        warn!("save_remote_track_metadata: null url");
        return -1;
    }

    let url = unsafe { CStr::from_ptr(url) };
    let url = match url.to_str() {
        Ok(url) => url,
        Err(e) => {
            warn!("save_remote_track_metadata: invalid utf-8: {}", e);
            return -1;
        }
    };

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            tracing::error!(
                "save_remote_track_metadata: failed to create runtime: {}",
                e
            );
            return -1;
        }
    };

    match rt.block_on(async {
        let pool = create_connection_pool().await?;
        save_audio_metadata(pool, url, None).await
    }) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!("save_remote_track_metadata: {}", e);
            -1
        }
    }
}
