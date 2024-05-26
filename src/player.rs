use std::fs::File;
use std::future::IntoFuture;
use std::io::BufReader;
use std::time::Duration;
use rodio::{Decoder, OutputStream, Sink};
use rodio::source::{SineWave, Source};
use crate::client::{self, Client};

use std::io::Cursor;
use std::io::Seek;
use std::io::Read;

use std::pin::Pin;
use std::task::{Context, Poll};
use bytes::Bytes;
use futures::{FutureExt, Stream};
use futures_util::StreamExt;
use futures_util::AsyncReadExt;
use futures_util::AsyncRead;
use std::error::Error;

use rodio::Sample;
// file
use std::io::Write; // bring trait into scope
use std::fs;

use rodio::buffer::SamplesBuffer;

pub struct Song {
    // pub audio_source: Vec<u8>,
    // the audio source has to be a stream, not a buffer
    pub audio_source: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
    pub position: usize,
    pub channels: u16,
    pub srate: u32,
    pub duration: Option<Duration>,
    pub file_size: u64,
    pub buffer: Vec<u8>,
    pub b: SamplesBuffer<i16>,
    pub file: Option<File>,
}

// impl Iterator for Song {
//     // write an async iterator that reads from the inner async stream
//     type Item = Result<Song, reqwest::Error>;
    
//     fn next(&mut self) -> Option<Self::Item> {
//         futures::executor::block_on(async {
//             let mut buf = vec![0u8; 1024];
//             //let mut audio_source = self.audio_source;
//             let mut position = self.position;
//             let mut channels = self.channels;
//             let mut srate = self.srate;
//             let mut duration = self.duration;
//             let mut file_size = self.file_size;

//             let mut audio_data = Vec::new();
//             let mut audio_data_len = 0;
//             let mut audio_data_size = 0;
//             let mut audio_data_offset = 0;

//             loop {
//                 // pin
//                 match self.audio_source.next().await {
//                     Some(Ok(bytes)) => {
//                         // println!("Got bytes: {:?}", bytes.len());
//                         audio_data.extend_from_slice(&bytes);
//                         audio_data_len += bytes.len();
//                         audio_data_size += bytes.len();
//                         // println!("Audio data size: {:?}", audio_data_size);
//                         // println!("Audio data len: {:?}", audio_data_len);
//                         // println!("Audio data offset: {:?}", audio_data_offset);
//                         // println!("File size: {:?}", file_size);
//                         if audio_data_size >= 1024 {
//                             break;
//                         }
//                     }
//                     Some(Err(e)) => {
//                         println!("Error: {:?}", e);
//                         return None::<Self::Item>;
//                     }
//                     None => {
//                         println!("End of stream");
//                         return None;
//                     }
//                 }
//             }
//             None
//         });
//         None
//     }

// }


impl Song {
    pub fn new(channels: u16, srate: u32, duration: Option<Duration>, file_size: u64) -> Self {
        Song {
            // empty stream
            audio_source: Box::pin(futures::stream::empty()),
            position: 0,
            channels,
            srate,
            duration,
            file_size,
            buffer: Vec::new(),
            b: SamplesBuffer::new(channels, srate, Vec::new()),
            file: None,
        }
    }
}

impl Iterator for Song {
    // type Item = Result<Bytes, Error>;
    type Item = i16;

    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer.len() < 1024 {
            println!("1Got bytes: {:?}", self.buffer.len());
            // read more data
            // println!("More bytes?: {:?}\n\n", self.buffer.len());
            let n = futures::executor::block_on(
                self.audio_source.next()
            );
            let n = match n {
                Some(Ok(n)) => n,
                Some(Err(e)) => {
                    println!("Error: {:?}", e);
                    return None;
                }
                None => {
                    println!("End of stream");
                    return None;
                }
            };
            println!("Got bytes: {:?}", n.len());
            self.buffer.extend(&n);
            // println!("Got bytes: {:?}\n\n", self.buffer);
        }
        
        // get our two u8
        let a = self.buffer[0] as u8;
        let b = self.buffer[1] as u8;
        // println!("a: {:?}, b: {:?}", a, b);
        // remove the first two bytes
        self.buffer = self.buffer[2..].to_vec();
        // println!("Got bytes: {:?}", self.buffer.len());
        // return the two bytes
        self.file.as_ref().unwrap().write_all(&[a, b]).unwrap();
        Some(i16::from_le_bytes([a, b]))
    }
}

impl Source for Song {
    fn current_frame_len(&self) -> Option<usize> {
        // Some((self.audio_source.len() - self.position) / 2)
        // use duration and position
        // println!("Duration: {:?}", self.duration.unwrap().as_secs_f32() * self.srate as f32);
        return Some(((self.duration.unwrap().as_secs_f32() - self.position as f32) * self.srate as f32) as usize);
    }

    fn channels(&self) -> u16 {
        self.channels
    }

    fn sample_rate(&self) -> u32 {
        self.srate
    }

    fn total_duration(&self) -> Option<Duration> {
        self.duration
    }
}

pub async fn mmain(client: &Client) {

    // _stream must live as long as the sink
    println!("Playing a sound...");
    let (_stream, stream_handle) = OutputStream::try_default().unwrap();
    let sink = Sink::try_new(&stream_handle).unwrap();

    let mut song_info = match client.song_info("0416871eb42dd5aa5c73da6930d6028e").await {
        Ok(s) => s,
        Err(e) => {
            println!("Error: {:?}", e);
            return;
        }
    };
    // println!("{:#?}", song_info);
    // set info is an arbitrary json, let's take out the audio metadata and create a Song

    println!("Loading sound...");

    let source = match client.stream().await {
        Ok(s) => {
            song_info.audio_source = Box::pin(s);
        }
        Err(e) => {
            println!("Error: {:?}", e);
            return;
        }
    };

    println!("Done loading sound...");

    // file for writing
    // let mut file = File::create("output2.mp3").unwrap();
    // while let Some(sample) = song_info.next() {
    //     // println!("{:?}", sample);
    //     // write to the file to try and reconstruct the audio
    //     let c = sample.to_le_bytes();
    //     file.write_all(&sample.to_le_bytes()).unwrap();
    // }

    song_info.file = Some(File::create("output2.mp3").unwrap());

    println!("{}", song_info.current_frame_len().unwrap().to_string());
    println!("{}", song_info.channels().to_string());
    println!("{}", song_info.sample_rate().to_string());
    println!("{}", song_info.total_duration().unwrap().as_secs_f32().to_string());
    // println!("{}", song_info.file_size.to_string());
    
    // Add a dummy source of the sake of the example.
    // let source = SineWave::new(440.0).take_duration(Duration::from_secs_f32(0.25)).amplify(0.20);
    // let source = rodio::Decoder::new(BufReader::new(audioSource)).unwrap();
    // sink.append(Decoder::new(source).unwrap());
    // set volue
    sink.append(song_info);

    // The sound plays in a separate thread. This call will block the current thread until the sink
    // has finished playing all its queued sounds.
    sink.sleep_until_end();
}