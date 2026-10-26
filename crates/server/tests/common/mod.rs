use nested_chess_server::store::Store;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;

pub async fn database() -> (String, Store) {
    let base = std::env::var("TEST_DATABASE_URL")
        .expect("Set TEST_DATABASE_URL to a real disposable PostgreSQL database.");
    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&base)
        .await
        .unwrap();
    let schema = format!("test_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let mut url = url::Url::parse(&base).unwrap();
    url.query_pairs_mut()
        .append_pair("options", &format!("-csearch_path={schema}"));
    let url = url.to_string();
    let store = Store::connect(&url).await.unwrap();
    (url, store)
}
