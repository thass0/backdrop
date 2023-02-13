use crate::helper::TestApp;

#[tokio::test]
async fn health_check_works() {
    let test_app = TestApp::spawn().await;
    let response = test_app.get_route("health_check").await;
    assert!(response.status().is_success());
    assert_eq!(Some(0), response.content_length());
}
