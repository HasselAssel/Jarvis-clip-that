use rwhisper::{WhisperBuilder, WhisperSource};
use std::sync::mpsc::Receiver;
use tokio_stream::wrappers::ReceiverStream;
use futures_util::StreamExt;

pub async fn transcribe_live(receiver: Receiver<Vec<i16>>) -> Result<()> {
    // Load the small English Whisper model
    let model = WhisperBuilder::default()
        .with_source(WhisperSource::SmallEn)
        .build()
        .await?;

    // Wrap your receiver in an async stream
    let stream = ReceiverStream::new(receiver)
        .map(|chunk| chunk.into_iter().map(f32::from).collect::<Vec<f32>>());

    // Start the transcription
    let mut transcription = model.transcribe_stream(stream);

    // Read live partial results
    while let Some(segment) = transcription.next().await {
        println!("Partial: {}", segment.text);
    }

    Ok(())
}




















/*use std::sync::mpsc::{channel, Receiver};
use std::thread;
use vosk::{Model, Recognizer};
use rodio::{Decoder, Source};
use std::io::Cursor;

fn main() {
    // Simulated audio source: replace with your real-time source
    let (tx, rx) = channel::<Vec<u8>>();

    // Spawn a thread to simulate streaming raw audio chunks
    thread::spawn(move || {
        loop {
            let fake_audio_data = vec![0u8; 4096]; // Replace with real PCM data
            tx.send(fake_audio_data).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    // Download a Vosk model from: https://alphacephei.com/vosk/models
    // Unpack it and place the folder (e.g., "vosk-model-small-en-us-0.15") in your project root as "model"
    let model = Model::new("model").expect("Failed to load model");
    let mut recognizer = Recognizer::new(&model, 16000.0).expect("Failed to create recognizer");

    listen_for_keywords(rx, &mut recognizer);
}

fn listen_for_keywords(rx: Receiver<Vec<u8>>, recognizer: &mut Recognizer) {
    while let Ok(audio_chunk) = rx.recv() {
        // Convert &[u8] to &[i16] for Vosk
        if audio_chunk.len() % 2 != 0 {
            eprintln!("Audio chunk has odd length");
            continue;
        }

        let samples: &[i16] = unsafe {
            std::slice::from_raw_parts(audio_chunk.as_ptr() as *const i16, audio_chunk.len() / 2)
        };

        if let Ok(_) = recognizer.accept_waveform(samples) {
            let result = recognizer.result();
            println!("Recognized: {:?}", result);
            // Here you can scan `result` for specific keywords
        } else {
            let partial = recognizer.partial_result();
            println!("Partial: {:?}", partial);
        }
    }
}
*/