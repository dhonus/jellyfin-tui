#[cfg(test)]
mod tests {
    use crate::client::Client;
    use crate::config::AuthEntry;
    use wiremock::{MockServer, Mock, ResponseTemplate};
    use wiremock::matchers::{method, path};

    #[tokio::test]
    async fn test_music_libraries_on_401_non_json() {
        let mock_server = MockServer::start().await;
        
        // Mock the Views endpoint to return 401 Unauthorized with non-JSON body
        Mock::given(method("GET"))
            .and(path("/Users/user1/Views"))
            .respond_with(ResponseTemplate::new(401).set_body_string("Unauthorized"))
            .mount(&mock_server)
            .await;

        let entry = AuthEntry {
            known_urls: vec![mock_server.uri()],
            device_id: "device1".to_string(),
            access_token: "token1".to_string(),
            user_id: "user1".to_string(),
            username: "username1".to_string(),
        };

        let client = Client::from_cache(&mock_server.uri(), &"server1".to_string(), &entry).await;
        
        let result = client.music_libraries().await;
        
        // It should return Ok(vec![]) because music_libraries handles the error
        assert!(result.is_ok());
        let libs = result.unwrap();
        assert!(libs.is_empty());
    }
}
