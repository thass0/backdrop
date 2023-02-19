use redis::{aio::Connection, AsyncCommands};
use crate::helper::get_redis_pool;


#[tokio::test]
async fn redis_connection_works() {
    let mut conn = get_redis_pool().get().await.unwrap();

    let response: String = redis::cmd("PING")
        .query_async(&mut conn as &mut Connection)
        .await
        .unwrap();
    assert_eq!(&response, "PONG");

}

#[tokio::test]
async fn redis_set_get_works() {
    let mut conn = get_redis_pool().get().await.unwrap();
    
    const TEST_KEY: &str = "test-key";
    const TEST_CONTENT: &str = "test-content";

    let _: () = conn.set(TEST_KEY, TEST_CONTENT).await.unwrap();
    let response: String = conn.get(TEST_KEY).await.unwrap();
    assert_eq!(response, TEST_CONTENT);    
}
