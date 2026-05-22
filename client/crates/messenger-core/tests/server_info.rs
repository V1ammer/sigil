use messenger_core::api::client::ApiClient;

#[tokio::test]
#[ignore = "requires running server"]
async fn test_server_info_via_client() {
    let client = ApiClient::new("http://127.0.0.1:8080");
    let info = client.server_info().await.unwrap();
    assert_eq!(info.mls_ciphersuite, 1);
}
