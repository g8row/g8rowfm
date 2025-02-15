mod metadata;
mod processor;

use futures_util::{StreamExt, TryStreamExt};
use metadata::TrackMetadata;
use processor::process_file_with_eos;
use std::{
    env, fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};
use tokio::{
    fs::{File, OpenOptions},
    io::AsyncWriteExt,
};
use warp::{
    filters::multipart::FormData,
    reject::Rejection,
    reply::{json, Reply},
    Buf, Filter,
};

const PLAYLIST_FILE: &str = "playlist.txt";

#[tokio::main]
async fn main() {
    if Path::new("segments").exists() {
        fs::remove_dir_all("segments").expect("Failed to delete segments folder");
    }
    fs::create_dir_all("segments").expect("Failed to create segments directory");

    let mut args = env::args().skip(1);
    let mut port_arg = None;
    let mut dir_arg = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => match args.next() {
                Some(value) => port_arg = Some(value),
                None => {
                    eprintln!("Error: --port requires a value");
                    return;
                }
            },
            "--dir" => match args.next() {
                Some(value) => dir_arg = Some(value),
                None => {
                    eprintln!("Error: --dir requires a value");
                    return;
                }
            },
            _ => {
                eprintln!("Error: Unknown argument '{}'", arg);
                return;
            }
        }
    }

    let port;
    let dir;

    match (port_arg, dir_arg) {
        (Some(p), Some(d)) => {
            port = p;
            dir = d;
        }
        (None, _) => {
            eprintln!("Error: Missing required argument --port");
            return;
        }
        (_, None) => {
            eprintln!("Error: Missing required argument --dir");
            return;
        }
    }

    let dir_clone = dir.clone();

    let restart_flag = Arc::new(AtomicBool::new(false));
    let current_track = Arc::new(Mutex::new(TrackMetadata::default()));
    let track_state = current_track.clone();

    let restart_clone = restart_flag.clone();

    thread::spawn(move || loop {
        let files = Arc::new(Mutex::new(load_playlist(&dir.clone())));
        process_file_with_eos(files.clone(), track_state.clone(), restart_clone.clone());
        thread::sleep(std::time::Duration::from_secs(1));
    });

    let playlist_ui =
        warp::path("playlist").map(|| warp::reply::html(include_str!("playlist.html")));

    let get_playlist = warp::path("api")
        .and(warp::path("playlist"))
        .and(warp::get())
        .map(|| {
            let content = fs::read_to_string(PLAYLIST_FILE).unwrap_or_default();
            warp::reply::json(&content.lines().collect::<Vec<_>>())
        });

    let save_playlist = warp::path("api")
        .and(warp::path("playlist"))
        .and(warp::put())
        .and(warp::body::bytes())
        .and_then(|bytes: warp::hyper::body::Bytes| async move {
            let content = String::from_utf8_lossy(&bytes).to_string();
            match tokio::fs::write(PLAYLIST_FILE, content).await {
                Ok(_) => {
                    println!("Playlist saved successfully.");
                    Ok::<_, warp::Rejection>(warp::reply::with_status(
                        warp::reply::json(&"Playlist updated"),
                        warp::http::StatusCode::OK,
                    ))
                }
                Err(e) => {
                    eprintln!("Failed to write playlist: {}", e);
                    Ok::<_, warp::Rejection>(warp::reply::with_status(
                        warp::reply::json(&"Failed to update playlist"),
                        warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                    ))
                }
            }
        });

    let restart = {
        let restart_flag = restart_flag.clone();
        warp::path("api")
            .and(warp::path("restart"))
            .and(warp::post())
            .map(move || {
                restart_flag.store(true, Ordering::SeqCst);
                warp::reply::json(&"Playback restarted with new playlist")
            })
    };

    let current_song = {
        let state = current_track.clone();
        warp::path("current-song").map(move || {
            let track = state.lock().unwrap();
            warp::reply::json(&track.title)
        })
    };

    let current_artist = {
        let state = current_track.clone();
        warp::path("current-artist").map(move || {
            let track = state.lock().unwrap();
            warp::reply::json(&track.artist)
        })
    };

    let current_album = {
        let state = current_track.clone();
        warp::path("current-album").map(move || {
            let track = state.lock().unwrap();
            warp::reply::json(&track.album)
        })
    };

    let current_cover = {
        let state = current_track.clone();
        warp::path("current-cover").map(move || {
            let track = state.lock().unwrap();
            warp::reply::json(&track.cover)
        })
    };

    let cors = warp::cors()
        .allow_any_origin()
        .allow_headers(vec![
            "User-Agent",
            "Sec-Fetch-Mode",
            "Referer",
            "Origin",
            "Access-Control-Request-Method",
            "Access-Control-Request-Headers",
            "Content-Type",
            "Accept",
            "Authorization",
            "content-type",
            "type",
        ])
        .allow_methods(vec!["GET", "POST", "DELETE", "OPTIONS"]);

    let hls_route = warp::path("hls")
        .and(warp::fs::dir("segments"))
        .with(warp::reply::with::header(
            "Content-Type",
            "application/vnd.apple.mpegurl",
        ))
        .with(warp::reply::with::header(
            "Cache-Control",
            "no-cache, no-store, must-revalidate",
        ))
        .with(warp::reply::with::header(
            "Access-Control-Allow-Origin",
            "*",
        ))
        .with(warp::reply::with::header(
            "Access-Control-Expose-Headers",
            "Content-Length",
        ));
    let upload_flac = warp::path("api")
        .and(warp::path("upload"))
        .and(warp::post())
        .and(warp::multipart::form().max_length(100_000_000))
        .and(warp::any().map(move || dir_clone.clone()))
        .and_then(handle_file_upload)
        .with(&cors);

    let routes = warp::path::end()
        .map(|| warp::reply::html(include_str!("index.html")))
        .or(hls_route)
        .or(current_song)
        .or(current_artist)
        .or(current_album)
        .or(current_cover)
        .or(restart)
        .or(playlist_ui)
        .or(get_playlist)
        .or(save_playlist)
        .or(upload_flac)
        .with(&cors);
    println!("Server running at http://localhost:{}", port);
    warp::serve(routes)
        .run(([127, 0, 0, 1], port.parse().unwrap()))
        .await;
}

async fn handle_file_upload(form: FormData, dir: String) -> Result<impl Reply, Rejection> {
    let mut parts = form.into_stream();
    while let Ok(part) = parts.next().await.unwrap() {
        if let Some(filename) = part.filename() {
            if !filename.ends_with(".flac") {
                return Ok(warp::reply::with_status(
                    json(&"Only FLAC files allowed"),
                    warp::http::StatusCode::BAD_REQUEST,
                ));
            }

            let mut filepath = format!("{}/{}", &dir, filename);
            let mut file = match File::create(&filepath).await {
                Ok(f) => f,
                Err(_) => {
                    return Ok(warp::reply::with_status(
                        json(&"Failed to create file"),
                        warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                    ));
                }
            };

            let mut stream = part.stream();
            while let Some(chunk) = stream.try_next().await.unwrap_or(None) {
                if file.write_all(&chunk.chunk()).await.is_err() {
                    return Ok(warp::reply::with_status(
                        json(&"Failed to write file"),
                        warp::http::StatusCode::INTERNAL_SERVER_ERROR,
                    ));
                }
            }
            let mut file = OpenOptions::new()
                .append(true) // Open the file in append mode
                .create(true) // Create the file if it doesn't exist
                .open(PLAYLIST_FILE)
                .await
                .unwrap();
            filepath.insert_str(0, "\n");
            file.write_all(filepath.as_bytes()).await.unwrap();

            return Ok(warp::reply::with_status(
                json(&"File uploaded successfully"),
                warp::http::StatusCode::OK,
            ));
        }
    }

    Ok(warp::reply::with_status(
        json(&"No file provided"),
        warp::http::StatusCode::BAD_REQUEST,
    ))
}

fn load_playlist(dir: &str) -> Vec<String> {
    if Path::new(PLAYLIST_FILE).exists() {
        if let Ok(content) = fs::read_to_string(PLAYLIST_FILE) {
            let files: Vec<String> = content
                .lines()
                .map(String::from)
                .filter(|f| Path::new(f).exists())
                .collect();
            if !files.is_empty() {
                return files;
            }
        }
    }
    let files = get_flac_files(dir);
    if !files.is_empty() {
        let _ = fs::write(PLAYLIST_FILE, files.join("\n"));
    }
    files
}

fn get_flac_files(dir: &str) -> Vec<String> {
    let mut files = vec![];
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |e| e == "flac") {
                if let Some(str_path) = path.to_str() {
                    files.push(str_path.to_string());
                }
            }
        }
    }
    files.sort();
    files
}
