use gstreamer::{message, prelude::*, ElementFactory};
use metadata::{extract_metadata, TrackMetadata};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

use crate::metadata;

pub fn process_file_with_eos(
    files: Arc<Mutex<Vec<String>>>,
    track_state: Arc<Mutex<TrackMetadata>>,
    restart_flag: Arc<AtomicBool>,
) {
    gstreamer::init().unwrap();

    let pipeline = gstreamer::Pipeline::new();

    let concat = ElementFactory::make("concat")
        .name("concat")
        .build()
        .unwrap();
    pipeline.add(&concat).unwrap();
    let bus = pipeline.bus().unwrap();
    let bus_ref = Arc::new(bus);

    let files_list = files.lock().unwrap().clone();
    if files_list.is_empty() {
        println!("No tracks found, waiting...");
        thread::sleep(std::time::Duration::from_secs(2));
        return;
    }

    for file in files_list.iter() {
        let filesrc = ElementFactory::make("filesrc")
            .property("location", file)
            .build()
            .unwrap();

        let flacparse = ElementFactory::make("flacparse").build().unwrap();
        let flacdec = ElementFactory::make("flacdec").build().unwrap();
        let audioconvert = ElementFactory::make("audioconvert").build().unwrap();
        let queue = ElementFactory::make("queue")
            .property("max-size-time", 5_000_000_000u64) // 2-second buffer
            .build()
            .unwrap();
        let identity = ElementFactory::make("identity").build().unwrap();

        pipeline
            .add_many(&[
                &filesrc,
                &flacparse,
                &flacdec,
                &queue,
                &audioconvert,
                &identity,
            ])
            .unwrap();

        gstreamer::Element::link_many(&[
            &filesrc,
            &flacparse,
            &flacdec,
            &queue,
            &audioconvert,
            &identity,
        ])
        .unwrap();

        let identity_src = identity.static_pad("src").unwrap();
        let concat_sink = concat.request_pad_simple("sink_%u").unwrap();
        identity_src.link(&concat_sink).unwrap();

        let bus_clone = Arc::clone(&bus_ref);
        let file_path = file.clone();
        let sent = Arc::new(AtomicBool::new(false));

        identity.static_pad("src").unwrap().add_probe(
            gstreamer::PadProbeType::BUFFER,
            move |_pad, _info| {
                if sent.load(Ordering::Relaxed) {
                    return gstreamer::PadProbeReturn::Ok;
                }
                sent.store(true, Ordering::Relaxed);

                let structure = gstreamer::Structure::builder("song-changed")
                    .field("file", file_path.as_str())
                    .build();
                let msg = message::Application::builder(structure)
                    .src(&identity)
                    .build();
                let _ = bus_clone.post(msg);

                gstreamer::PadProbeReturn::Ok
            },
        );
    }

    let queue = ElementFactory::make("queue")
        .property("max-size-time", 5_000_000_000u64) // 2-second buffer
        .build()
        .unwrap();
    let post_convert = ElementFactory::make("audioconvert").build().unwrap();
    let resample = ElementFactory::make("audioresample")
        .property("quality", 1i32) // Quality/speed tradeoff
        .build()
        .unwrap();
    let rate = ElementFactory::make("audiorate").build().unwrap();
    let identity = ElementFactory::make("identity")
        .property("sync", true)
        .build()
        .unwrap();
    let queue2 = ElementFactory::make("queue")
        .property("max-size-time", 5_000_000_000u64) // 2-second buffer
        .build()
        .unwrap();
    let encoder = ElementFactory::make("avenc_ac3")
        .property("bitrate", 128000i32)
        .build()
        .unwrap();
    //browser doesnt work with high bitrate/aac, other clients work

    let _encoder = ElementFactory::make("avenc_aac")
        .property("bitrate", 128000i32) // 128 kbps
        .build()
        .unwrap();

    let hlssink = ElementFactory::make("hlssink2")
        .property("location", "segments/segment%05d.ts")
        .property("playlist-location", "segments/playlist.m3u8")
        .property("target-duration", 1u32)
        .property("max-files", 5u32)
        .property("playlist-length", 5u32)
        .build()
        .unwrap();

    pipeline
        .add_many(&[
            &queue,
            &post_convert,
            &resample,
            &rate,
            &identity,
            &queue2,
            &encoder,
            &hlssink,
        ])
        .unwrap();

    gstreamer::Element::link_many(&[
        &concat,
        &queue,
        &post_convert,
        &resample,
        &rate,
        &identity,
        &queue2,
        &encoder,
        &hlssink,
    ])
    .unwrap();

    pipeline.set_state(gstreamer::State::Playing).unwrap();

    loop {
        match bus_ref.timed_pop(gstreamer::ClockTime::NONE) {
            Some(msg) => match msg.view() {
                gstreamer::MessageView::Application(app) => {
                    let structure_o = app.structure();
                    match structure_o {
                        Some(structure) => {
                            if structure.name() == "song-changed" {
                                let file = structure.get::<&str>("file").unwrap();
                                let metadata = extract_metadata(file);

                                let mut track = track_state.lock().unwrap();
                                *track = metadata;
                                println!("Now playing: {} - {}", track.title, track.artist);
                            }
                        }
                        None => continue,
                    }
                }
                gstreamer::MessageView::Eos(_) => {
                    println!("End of stream");
                    break;
                }
                gstreamer::MessageView::Error(err) => {
                    eprintln!("Error: {}", err.error());
                    break;
                }
                _ => (),
            },
            None => continue,
        }
        if restart_flag.load(Ordering::SeqCst) {
            println!("Restarting playback with new playlist...");
            restart_flag.store(false, Ordering::SeqCst);
            pipeline.set_state(gstreamer::State::Null).unwrap();
            return;
        }
    }

    pipeline.set_state(gstreamer::State::Null).unwrap();
}
