use gstreamer::{parse::launch, prelude::*};
use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use warp::Filter;

#[tokio::main]
async fn main() {
    fs::create_dir_all("segments").expect("Failed to create segments directory");

    let music_dir = "/home/alex/Downloads/music";
    let files = Arc::new(get_flac_files(music_dir));
    if files.is_empty() {
        panic!("No FLAC files found in {}", music_dir);
    }

    let current_index = Arc::new(Mutex::new(0));
    let segment_counter = Arc::new(Mutex::new(0));
    let preparing_next = Arc::new(Mutex::new(false));

    let gst_files = Arc::clone(&files);
    let gst_index = Arc::clone(&current_index);
    let gst_segments = Arc::clone(&segment_counter);
    let gst_next_flag = Arc::clone(&preparing_next);

    thread::spawn(move || {
        gstreamer::init().unwrap();
        run_gstreamer_pipeline(gst_files, gst_index, gst_segments, gst_next_flag);
    });

    let hls = warp::path("hls").and(warp::fs::dir("segments"));
    let routes = hls;

    println!("Server running at http://localhost:8080");
    warp::serve(routes).run(([127, 0, 0, 1], 8080)).await;
}

fn get_flac_files(dir: &str) -> Vec<String> {
    let path = Path::new(dir);
    let mut files = Vec::new();

    if let Ok(entries) = fs::read_dir(path) {
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

fn run_gstreamer_pipeline(
    files: Arc<Vec<String>>,
    current_index: Arc<Mutex<usize>>,
    segment_counter: Arc<Mutex<usize>>,
    preparing_next: Arc<Mutex<bool>>,
) {
    loop {
        let index = *current_index.lock().unwrap();
        let current_file = &files[index];

        println!("Starting track: {}", current_file);

        let playlist_path = "segments/playlist.m3u8";
        let segment_prefix = "segments/segment";

        launch_pipeline(current_file, playlist_path, segment_prefix);

        let segment_duration = Duration::from_secs(10);
        let song_duration = get_song_duration(current_file);

        let mut elapsed_time = Duration::ZERO;

        while elapsed_time < song_duration {
            thread::sleep(segment_duration);
            elapsed_time += segment_duration;
            update_playlist(playlist_path, &mut *segment_counter.lock().unwrap());

            if elapsed_time + (segment_duration * 5) >= song_duration {
                let mut next_flag = preparing_next.lock().unwrap();
                if !*next_flag {
                    *next_flag = true;
                    let next_index = (index + 1) % files.len();
                    let next_file = Arc::clone(&files);
                    println!("Pre-generating next track: {}", next_file[next_index]);

                    let next_playlist_path = "segments/next_playlist.m3u8";
                    let next_segment_prefix = "segments/next_segment";

                    let next_file_path = next_file[next_index].clone();
                    thread::spawn(move || {
                        launch_pipeline(&next_file_path, next_playlist_path, next_segment_prefix);
                    });
                }
            }
        }

        *preparing_next.lock().unwrap() = false;

        println!("Switching to next track...");
        fs::rename("segments/next_playlist.m3u8", "segments/playlist.m3u8")
            .expect("Failed to swap playlists");

        *current_index.lock().unwrap() = (index + 1) % files.len();
    }
}

fn launch_pipeline(file: &str, playlist: &str, segment_prefix: &str) {
    let pipeline_str = format!(
        "filesrc location=\"{}\" ! \
        flacparse ! flacdec ! \
        audioconvert ! audioresample ! \
        avenc_ac3 bitrate=640000 ! \
        hlssink2 name=hlssink \
        location={}%05d.ts \
        playlist-location={} \
        target-duration=10 \
        max-files=10000 \
        playlist-length=10000",
        file, segment_prefix, playlist
    );

    let pipeline = launch(&pipeline_str).expect("Failed to parse pipeline");
    pipeline.set_state(gstreamer::State::Playing).unwrap();

    println!("GStreamer pipeline running for: {}", file);
}

fn get_song_duration(file_path: &str) -> Duration {
    use lofty::{prelude::AudioFile, probe::Probe};

    let tagged_file = Probe::open(file_path)
        .expect("Failed to open file")
        .read()
        .expect("Failed to read metadata");

    let properties = tagged_file.properties();
    return Duration::from_secs(properties.duration().as_secs());
}

fn update_playlist(playlist_path: &str, segment_counter: &mut usize) {
    let playlist = fs::read_to_string(playlist_path).unwrap_or_else(|_| "".to_string());
    let mut lines: Vec<&str> = playlist.lines().collect();

    let segment_lines: Vec<&str> = lines
        .iter()
        .filter(|line| line.contains(".ts"))
        .cloned()
        .collect();

    let max_segments = 10;
    let min_segments = 5;

    if segment_lines.len() > max_segments {
        let remove_count = segment_lines.len() - min_segments;

        for line in &segment_lines[..remove_count] {
            if let Some(ts_file) = line.strip_prefix("#EXTINF:") {
                let ts_path = format!("segments/{}", ts_file.trim());
                if fs::remove_file(&ts_path).is_ok() {
                    println!("Removed old segment: {}", ts_path);
                }
            }
        }

        lines = lines[remove_count * 2..].to_vec();
    }

    let new_segment = format!("segment{:05}.ts", *segment_counter);
    *segment_counter += 1;

    lines.push("#EXTINF:10,");
    lines.push(&new_segment);

    fs::write(playlist_path, lines.join("\n")).expect("Failed to update playlist");
    println!("Updated playlist with new segment: {}", new_segment);
}
