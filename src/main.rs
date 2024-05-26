use reqwest::Error;
use tokio;
pub mod client;
mod player;

#[tokio::main]
async fn main() {
    let client = client::Client::new("https://jelly.danielhonus.com").await;
    if client.access_token.is_empty() {
        println!("Failed to authenticate. Exiting...");
        return;
    }
    //client.songs().await;
    // let artists = client.artists().await;
    // let songs = client.songs().await;

    player::mmain(&client).await;
}
