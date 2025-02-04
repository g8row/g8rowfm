use gstreamer::{parse::launch, prelude::*};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};
use warp::Filter;

#[tokio::main]
async fn main() {
    // Create segments directory
    std::fs::create_dir_all("segments").expect("Failed to create segments directory");

    // Get FLAC files from music directory
    let music_dir = "/home/alex/Downloads/music";
    let files = get_flac_files(music_dir);
    if files.is_empty() {
        panic!("No FLAC files found in {}", music_dir);
    }

    // Shared index for track cycling
    let current_index = Arc::new(Mutex::new(0));

    // Start GStreamer pipeline in a separate thread
    let gst_files = files.clone();
    let gst_index = current_index.clone();
    std::thread::spawn(move || {
        gstreamer::init().unwrap();
        run_gstreamer_pipeline(gst_files, gst_index);
    });

    // Serve static files and HLS segments
    let index = warp::path::end().map(|| warp::reply::html(include_str!("index.html")));

    let hls = warp::path("hls").and(warp::fs::dir("segments"));

    let routes = index.or(hls);

    println!("Server running at http://localhost:8080");
    warp::serve(routes).run(([127, 0, 0, 1], 8080)).await;
}

fn get_flac_files(dir: &str) -> Vec<String> {
    let path = Path::new(dir);
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(path) {
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

fn run_gstreamer_pipeline(files: Vec<String>, current_index: Arc<Mutex<usize>>) {
    let pipeline_str = "filesrc name=src ! \
        flacparse ! flacdec ! \
        audioconvert ! audioresample ! \
        avenc_ac3 bitrate=640000 ! \
        hlssink2 name=hlssink \
        location=segments/segment%05d.ts  \
        playlist-location=segments/playlist.m3u8 \
        target-duration=10 \
        max-files=10000 \
        playlist-length=10000 ";

    //let pipeline = Pipeline::new();

    let pipeline = launch(pipeline_str).expect("Failed to parse pipeline");
    let filesrc = pipeline
        .downcast_ref::<gstreamer::Bin>()
        .unwrap()
        .by_name("src")
        .unwrap();
    let sink = pipeline
        .downcast_ref::<gstreamer::Bin>()
        .unwrap()
        .by_name("hlssink")
        .unwrap();

    sink.set_property_from_str("playlist-root", "http://localhost:8080/hls");
    sink.set_property("playlist-length", &10u32);
    sink.set_property("target-duration", &10u32);

    let bus = pipeline.bus().unwrap();
    //pipeline.set_state(gstreamer::State::Paused).unwrap();

    //loop {
    let mut index = current_index.lock().unwrap();
    let current_file = &files[*index];

    filesrc.set_property_from_str("location", current_file);
    pipeline.set_state(gstreamer::State::Playing).unwrap();
    println!("Now playing: {}", current_file);

    // Handle messages
    let mut error = false;
    loop {
        match bus.timed_pop(gstreamer::ClockTime::NONE) {
            Some(msg) => match msg.view() {
                gstreamer::MessageView::Eos(..) => {
                    println!("Finished playing: {}", current_file);
                    break;
                }
                gstreamer::MessageView::Error(err) => {
                    eprintln!(
                        "Error from {:?}: {} ({:?})",
                        err.src().map(|s| s.path_string()),
                        err.error(),
                        err.debug()
                    );
                    error = true;
                    break;
                }
                _ => (),
            },
            None => continue, // No message yet, keep waiting
        }
        //}

        //pipeline.set_state(gstreamer::State::Null).unwrap();

        if error {
            break;
        }

        // Update index with wrap-around
        *index = (*index + 1) % files.len();
    }
    loop {}
    //pipeline.set_state(gstreamer::State::Null).unwrap();
}
