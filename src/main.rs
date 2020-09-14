use std::path::PathBuf;

use actix_web::{
    delete,
    http::HeaderValue,
    middleware::Logger,
    patch, post, put,
    web::{Data, Payload},
    App, HttpRequest, HttpServer, Responder,
};
use bb8_postgres::{
    bb8::Pool,
    tokio_postgres::{config::Config, NoTls},
    PostgresConnectionManager,
};
use futures::StreamExt;
use hyper::{client::HttpConnector, Body, Client, Request};
use hyper_rustls::HttpsConnector;
use lazy_static::lazy_static;
use std::str::FromStr;
use tokio::{
    fs::{remove_dir_all, rename, DirBuilder, File, OpenOptions},
    io::AsyncWriteExt,
};
use uuid::Uuid;

mod auth;

type PgPool = Pool<PostgresConnectionManager<NoTls>>;

lazy_static! {
    static ref CLIENT: Client<HttpsConnector<HttpConnector>, Body> = {
        let https = HttpsConnector::new();
        Client::builder().build::<_, Body>(https)
    };
    static ref BASE_URL: String = std::env::var("BASE_URL").expect("BASE_URL not set");
    static ref UPLOAD_DIRECTORY: String =
        std::env::var("UPLOAD_DIRECTORY").expect("UPLOAD_DIRECTORY not set");
    static ref ENABLE_CF: bool = std::env::var("CF_ID").is_ok();
    static ref CF_ID: String = std::env::var("CF_ID").expect("CF_ID not set");
    static ref CF_EMAIL: String = std::env::var("CF_EMAIL").expect("CF_EMAIL not set");
    static ref CF_KEY: String = std::env::var("CF_KEY").expect("CF_KEY not set");
}

async fn purge_cache(url: &str) {
    if !*ENABLE_CF {
        return;
    }
    let mut req = Request::builder()
        .method("POST")
        .uri(format!(
            "https://api.cloudflare.com/client/v4/zones/{}/purge_cache",
            *CF_ID
        ))
        .body(Body::from(format!("{{\"files\": [{:?}]}}", url)))
        .unwrap();
    let headers_mut = req.headers_mut();
    headers_mut.insert("X-Auth-Email", HeaderValue::from_static(&*CF_EMAIL));
    headers_mut.insert("X-Auth-Key", HeaderValue::from_static(&*CF_KEY));
    headers_mut.insert("Content-Type", HeaderValue::from_static("application/json"));
    let res = (*CLIENT).request(req).await.expect("Error requesting CF");
    assert_eq!(res.status(), 200);
}

fn parse_filename_from_uri(uri: &str) -> Option<String> {
    let mut path_base = PathBuf::new();
    path_base.push(uri);
    match path_base.file_name() {
        Some(file_name) => Some(file_name.to_str().unwrap().to_string()),
        None => None,
    }
}

#[post("*", wrap = "auth::RequiresAuth")]
async fn upload_file(req: HttpRequest, mut payload: Payload, pool: Data<PgPool>) -> impl Responder {
    let file_name = match parse_filename_from_uri(&req.uri().to_string()) {
        Some(n) => n,
        None => return String::from("No valid path given"),
    };
    let target_uuid = Uuid::new_v4().to_string();
    let mut base_path = PathBuf::new();
    base_path.push(UPLOAD_DIRECTORY.clone());
    base_path.push(target_uuid.clone());
    let directory = base_path.clone().to_str().unwrap().to_string();
    base_path.push(file_name.clone());
    let file_path = base_path.to_str().unwrap().to_string();
    DirBuilder::new()
        .recursive(true)
        .create(directory)
        .await
        .expect("Error creating directory");
    let mut file = File::create(file_path.clone())
        .await
        .expect("Error opening file");
    let display_path = format!("{}/{}", target_uuid, file_name);
    let conn = pool.get().await.unwrap();
    conn.execute(
        r#"INSERT INTO uploads ("file_path", "uploader") VALUES ($1, (SELECT "id" FROM users WHERE "key"=$2));"#,
        &[
            &display_path,
            &req.headers().get("Authorization").unwrap().to_str().unwrap()
        ]
    ).await.unwrap();
    while let Some(chunk) = payload.next().await {
        file.write_all(&chunk.expect("Error reading chunk"))
            .await
            .expect("Error writing to file");
    }
    format!("{}/{}", *BASE_URL, display_path)
}

#[put("*", wrap = "auth::RequiresOwnership")]
async fn overwrite_file(req: HttpRequest, mut payload: Payload) -> impl Responder {
    let target_path = &req.path()[1..];
    let mut base_path = PathBuf::new();
    base_path.push(UPLOAD_DIRECTORY.clone());
    base_path.push(target_path);
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(base_path)
        .await
        .expect("Error opening file");
    while let Some(chunk) = payload.next().await {
        file.write_all(&chunk.expect("Error reading chunk"))
            .await
            .expect("Error writing to file");
    }
    let target_url = format!("{}/{}", *BASE_URL, target_path);
    purge_cache(&target_url).await;
    target_url
}

#[delete("*", wrap = "auth::RequiresOwnership")]
async fn delete_file(req: HttpRequest, pool: Data<PgPool>) -> impl Responder {
    let target_path = &req.path()[1..];
    let mut base_path = PathBuf::new();
    base_path.push(UPLOAD_DIRECTORY.clone());
    base_path.push(target_path.split("/").next().unwrap());
    let conn = pool.get().await.unwrap();
    conn.execute(
        r#"DELETE FROM uploads WHERE "file_path"=$1;"#,
        &[&target_path],
    )
    .await
    .unwrap();
    remove_dir_all(base_path)
        .await
        .expect("Failed to remove directory");
    let target_url = format!("{}/{}", *BASE_URL, target_path);
    purge_cache(&target_url).await;
    "OK"
}

#[patch("*", wrap = "auth::RequiresOwnership")]
async fn move_file(req: HttpRequest, pool: Data<PgPool>) -> impl Responder {
    let rename_file_to = match req.headers().get("X-Rename-To") {
        Some(n) => n.to_str().unwrap(),
        None => return String::from("X-Rename-To not set"),
    };
    let name = match parse_filename_from_uri(rename_file_to) {
        Some(n) => n,
        None => return String::from("No valid path given"),
    };
    let target_path = &req.path()[1..];
    let mut base_path = PathBuf::new();
    let mut current_name_split = target_path.split("/");
    let uuid = current_name_split.next().unwrap();
    let current_file_name = current_name_split.next().unwrap();
    base_path.push(UPLOAD_DIRECTORY.clone());
    base_path.push(uuid);
    let mut current_path = base_path.clone();
    current_path.push(current_file_name);
    base_path.push(name.clone());
    let new_short = format!("{}/{}", uuid, name);
    let conn = pool.get().await.unwrap();
    conn.execute(
        r#"UPDATE uploads SET "file_path"=$1 WHERE "file_path"=$2;"#,
        &[&new_short, &target_path],
    )
    .await
    .unwrap();
    rename(current_path, base_path)
        .await
        .expect("Error renaming file");
    let old_url = format!("{}/{}", *BASE_URL, target_path);
    purge_cache(&old_url).await;
    format!("{}/{}", *BASE_URL, new_short)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    std::env::set_var("RUST_LOG", "actix_web=debug,actix_server=info");
    env_logger::init();

    let config = Config::from_str(&std::env::var("DATABASE_URL").unwrap()).unwrap();
    let manager = PostgresConnectionManager::new(config, NoTls);
    let pool = Pool::builder().build(manager).await.unwrap();

    HttpServer::new(move || {
        App::new()
            .wrap(Logger::default())
            .data(pool.clone())
            .service(upload_file)
            .service(overwrite_file)
            .service(delete_file)
            .service(move_file)
    })
    .bind("0.0.0.0:5006")?
    .run()
    .await
}
