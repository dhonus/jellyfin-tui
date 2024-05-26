// this file contains the jellyfin client module. We will use this module to interact with the jellyfin server.
// The client module will contain the following:
// 1. A struct that will hold the base url of the jellyfin server.
// 2. A function that will create a new instance of the client struct.
// 3. A function that will get the server information.
// 4. A function that will get the server users.
// 5. A function that will get the server libraries.

use std::fmt::format;
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;
use reqwest;
use serde::Serialize;
use serde::Deserialize;
use serde_json::Value;

use std::io::Cursor;
use std::io::Seek;

use crate::player::{self, Song};

use futures_util::StreamExt;

use std::pin::Pin;
use std::task::{Context, Poll};
use bytes::Bytes;
use futures::{Stream};

use serde_yaml;

pub struct ByteStream {
    inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
}

#[derive(Debug)]
pub struct Client {
    base_url: String,
    http_client: reqwest::Client,
    credentials: Option<Credentials>,
    pub access_token: String,
    user_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(rename = "Username")]
    username: String,
    #[serde(rename = "Pw")]
    password: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerInfo {
    version: String,
    url: String,
}

impl Client {
    pub async fn new(base_url: &str) -> Self {
        let f = std::fs::File::open("config.yaml").unwrap();
        let d: Value = serde_yaml::from_reader(f).unwrap();        

        let http_client = reqwest::Client::new();
        let _credentials = {
            // let username = std::env::var("").ok();
            // let password = std::env::var("").ok();
            let username = d["username"].as_str();
            let password = d["password"].as_str();
            match (username, password) {
                (Some(username), Some(password)) => Some(Credentials {
                    username: username.to_string(),
                    password: password.to_string(),
                }),
                _ => None,
            }
        };
        
        // println!("{}", format!("{}/Users/authenticatebyname", d["host"]).as_str());
        // without the ""
        let url: String = String::new() + &d["host"].as_str().unwrap() + "/Users/authenticatebyname";
        let response = http_client
            .post(url)
            .header("Content-Type", "text/json")
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            // .json(&Credentials {
            //     username: "".to_string(),
            //     password: "".to_string(),
            // })
            .json(&serde_json::json!({
                "Username": d["username"].as_str().unwrap(),
                "Pw": d["password"].as_str().unwrap()
            }))
            .send()
            .await;

        // check status without moving
        let status = response.as_ref().unwrap().status();
        if !status.is_success() {
            println!("Error authenticating. Status: {}", status);
            return Self {
                base_url: base_url.to_string(),
                http_client,
                credentials: _credentials,
                access_token: "".to_string(),
                user_id: "".to_string(),
            };
        }
            
        // get response data
        let response: Value = response.unwrap().json().await.unwrap();
        // get AccessToken
        let access_token = response["AccessToken"].as_str().unwrap();
        println!("Access Token: {}", access_token);

        // get user id (User.Id)
        let user_id = response["User"]["Id"].as_str().unwrap();
        println!("User Id: {}", user_id);


        println!("{:#?}", response);
        Self {
            base_url: base_url.to_string(),
            http_client,
            credentials: _credentials,
            access_token: access_token.to_string(),
            user_id: user_id.to_string(),
        }
    }

    pub async fn artists(&self) -> Result<Value, reqwest::Error> {
        // let url = format!("{}/Users/{}/Artists", self.base_url, self.user_id);
        let url = format!("{}/Artists", self.base_url);
        println!("url: {}", url);


        // to send some credentials we can use the basic_auth method
        // let response = self.http_client.get(url).basic_auth(&self.credentials.username, Some(&self.credentials.password)).send().await;
        let s = format!("MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\" Token=\"{}\"", self.access_token);
        println!("s: {}", s);
        let response: Result<reqwest::Response, reqwest::Error> = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .query(&[
                ("SortBy", "SortName"),
                ("SortOrder", "Ascending"), 
                ("Recursive", "true"), 
                ("Fields", "SortName"), 
                ("ImageTypeLimit", "-1")
            ])
            .query(&[("StartIndex", "0")])
            .query(&[("Limit", "100")])
            .send()
            .await;

        // check status without moving
        let status = response.as_ref().unwrap().status();

        // check if response is ok
        if !response.as_ref().unwrap().status().is_success() {
            println!("Error getting artists. Status: {}", status);
            return Ok(serde_json::json!({}));
        }

        // artists is the json 
        let artists: Value = response.unwrap().json().await.unwrap();
        
        // println!("{:#?}", artists);

        Ok(artists)
    }

    // get json schema of all artists
    // url/Artists?enableImages=true&enableTotalRecordCount=true
    pub async fn songs(&self) -> Result<Value, reqwest::Error> {
        let url = format!("{}/Users/{}/Items", self.base_url, self.user_id);
        // let url = format!("{}/Songs", self.base_url);
        println!("url: {}", url);


        // to send some credentials we can use the basic_auth method
        // let response = self.http_client.get(url).basic_auth(&self.credentials.username, Some(&self.credentials.password)).send().await;
        let s = format!("MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\" Token=\"{}\"", self.access_token);
        println!("s: {}", s);
        let response: Result<reqwest::Response, reqwest::Error> = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            // ?SortBy=Album%2CSortName&SortOrder=Ascending&IncludeItemTypes=Audio&Recursive=true&Fields=ParentId&StartIndex=0&ImageTypeLimit=1&EnableImageTypes=Primary
            .query(&[("SortBy", "Album,SortName"), ("SortOrder", "Ascending"), ("IncludeItemTypes", "Audio"), ("Recursive", "true"), ("Fields", "ParentId"), ("StartIndex", "0"), ("ImageTypeLimit", "1"), ("EnableImageTypes", "Primary")])
            .query(&[("Limit", "100")])
            .query(&[("StartIndex", "0")])
            .send()
            .await;

        // check status without moving
        let status = response.as_ref().unwrap().status();

        // check if response is ok
        if !response.as_ref().unwrap().status().is_success() {
            println!("Error getting artists. Status: {}", status);
            return Ok(serde_json::json!({}));
        }

        // artists is the json string of all artists
        let songs: Value = response.unwrap().json().await.unwrap();
        
        // println!("{:#?}", songs);

        Ok(songs)
    }

    pub async fn song_info(&self, song_id: &str) -> Result<Song, reqwest::Error> {
        let url = format!("{}/Items/{}", self.base_url, song_id);
        println!("url: {}", url);

        let response: Result<reqwest::Response, reqwest::Error> = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "text/json")
            .send()
            .await;

        // check status without moving
        let status = response.as_ref().unwrap().status();

        // check if response is ok
        if !response.as_ref().unwrap().status().is_success() {
            println!("Error getting artists. Status: {}", status);
            return Ok(Song::new(0, 0, None, 0));
        }

        // artists is the json string of all artists
        let song_info: Value = response.unwrap().json().await.unwrap();
        
        // println!("SONG INFO{:#?}", song_info);

        let channels = song_info["MediaStreams"][0]["Channels"].as_u64().unwrap() as u16;
        let srate = song_info["MediaStreams"][0]["SampleRate"].as_u64().unwrap() as u32;
        let duration = song_info["RunTimeTicks"].as_u64().unwrap() as u64;
        let file_size = song_info["MediaSources"][0]["Size"].as_u64().unwrap() as u64;
        println!("Channels: {}", channels);
        println!("Sample Rate: {}", srate);
        println!("Duration: {}", duration);
        println!("File Size in bytes: {}", file_size);
        println!("File Size in MB: {}", file_size / 1024 / 1024);

        Ok(Song::new(channels, srate, Some(std::time::Duration::from_secs(duration / 10000000)), file_size))
    }

    pub async fn stream(&self) -> Result<Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>, reqwest::Error> {
        let url = format!("{}/Audio/{}/universal", self.base_url, "0416871eb42dd5aa5c73da6930d6028e");
        println!("url: {}", url);

        // get song info
    
        let s = format!("MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\" Token=\"{}\"", self.access_token);
        println!("s: {}", s);
    
        let response = self.http_client
            .get(url)
            .header("X-MediaBrowser-Token", self.access_token.to_string())
            .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
            .header("Content-Type", "application/json")
            .query(&[
                ("UserId", self.user_id.to_string()),
                ("Container", "opus,webm|opus,mp3,aac,m4a|aac,m4b|aac,flac,webma,webm|webma,wav,ogg".to_string()),
                ("TranscodingContainer", "mp4".to_string()),
                ("TranscodingProtocol", "hls".to_string()),
                ("AudioCodec", "aac".to_string()),
                ("api_key", self.access_token.to_string()),
                ("StartTimeTicks", "0".to_string()),
                ("EnableRedirection", "true".to_string()),
                ("EnableRemoteMedia", "false".to_string())
            ])
            .send()
            .await?;
    
        let status = response.status();
    
        if !status.is_success() {
            println!("Error getting artists. Status: {}", status);
            // return Ok(Cursor::new(Arc::new([])));
            return Ok(Box::pin(futures::stream::empty()));
        } else {
            println!("Success getting audio stream. Status: {}", status);
        }
    
        //let content = vec![];
        // now we need to stream the data. For debugging just make a loop and print the data
        let mut stream = response.bytes_stream();
        // while let Some(item) = stream.next().await {
        //     // println!("Chunk: {:?}", item?);
        //     //println!("Chunk size: {:?}", item?.len());
        // }
        //println!("Content: {:?}", content.len());
        // Ok(Cursor::new(Arc::from(content.as_ref())))
        Ok(Box::pin(stream))

        // this is nice, but it gets the entire file at once. We need to stream it! So here returns a cursor that will stream the data. We can't just call .bytes() on the response because it will consume the response. We need to stream the data.
        // let content = response.bytes().await?; // this is bad
        // let content = response.bytes().await?;
        // Ok(Cursor::new(Arc::from(content.as_ref())))
    }

    // pub async fn stream(buffer: Arc<Mutex<StreamBuffer>>, base_url: &str, access_token: &str, user_id: &str, http_client: &reqwest::Client) -> Result<(), reqwest::Error> {
    //     let url = format!("{}/Audio/{}/universal", base_url, "2f039eccf11d82f21a2b74a6954ddef2");
    //     println!("url: {}", url);

    //     let response = http_client
    //         .get(&url)
    //         .header("X-MediaBrowser-Token", access_token.to_string())
    //         .header("x-emby-authorization", "MediaBrowser Client=\"jellyfin-tui\", Device=\"jellyfin-tui\", DeviceId=\"None\", Version=\"10.4.3\"")
    //         .header("Content-Type", "text/json")
    //         .query(&[
    //             ("UserId", user_id.to_string()),
    //             ("Container", "opus,webm|opus,mp3,aac,m4a|aac,m4b|aac,flac,webma,webm|webma,wav,ogg".to_string()),
    //             ("TranscodingContainer", "mp4".to_string()),
    //             ("TranscodingProtocol", "hls".to_string()),
    //             ("AudioCodec", "aac".to_string()),
    //             ("api_key", access_token.to_string()),
    //             ("StartTimeTicks", "0".to_string()),
    //             ("EnableRedirection", "true".to_string()),
    //             ("EnableRemoteMedia", "false".to_string())
    //         ])
    //         .send()
    //         .await?;

    //     if !response.status().is_success() {
    //         println!("Error getting audio stream. Status: {}", response.status());
    //         return Ok(());
    //     }

    //     let mut stream_buffer = buffer.lock().unwrap();
    //     let content = response.bytes().await?;

    //     for &byte in content.iter() {
    //         stream_buffer.data.push_back(byte);
    //     }

    //     Ok(())
    // }


}