use crate::helper::TestApp;

#[tokio::test]
async fn save_file_get_works() {
    let test_app = TestApp::spawn().await;
    let response = test_app.get_route("").await;
    assert!(response.status().is_success());
}