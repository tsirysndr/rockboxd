use std::path::PathBuf;

use actix_cors::Cors;
use actix_files::{self as fs, NamedFile};
use actix_web::{
    error::ErrorNotFound,
    guard,
    http::header::{ContentDisposition, DispositionType, HOST},
    web::{self, Data},
    App, HttpRequest, HttpResponse, HttpServer, Result,
};
use anyhow::Error;
use async_graphql::{http::GraphiQLSource, Schema};
use async_graphql_actix_web::{GraphQLRequest, GraphQLResponse, GraphQLSubscription};
use rockbox_library::{create_connection_pool, repo};
use rockbox_playlists::PlaylistStore;
use rockbox_webui::{dist, index, index_spa};
use sqlx::{Pool, Sqlite};

use crate::{
    schema::{Mutation, Query, Subscription},
    RockboxSchema,
};

/// Worker threads for this server — see `rockbox_server::http::workers` for
/// why the daemon caps them instead of taking actix's one-per-CPU default.
fn http_workers() -> usize {
    std::env::var("ROCKBOX_HTTP_WORKERS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get().min(4))
                .unwrap_or(4)
        })
}

async fn index_ws(
    schema: web::Data<RockboxSchema>,
    req: HttpRequest,
    payload: web::Payload,
) -> Result<HttpResponse> {
    GraphQLSubscription::new(Schema::clone(&*schema)).start(&req, payload)
}

#[actix_web::post("/graphql")]
async fn index_graphql(schema: web::Data<RockboxSchema>, req: GraphQLRequest) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

#[actix_web::get("/graphiql")]
async fn index_graphiql(req: HttpRequest) -> Result<HttpResponse> {
    let host = req
        .headers()
        .get(HOST)
        .unwrap()
        .to_str()
        .unwrap()
        .split(":")
        .next()
        .unwrap();

    let http_port = std::env::var("ROCKBOX_GRAPHQL_PORT").unwrap_or("6062".to_string());
    let graphql_endpoint = format!("http://{}:{}/graphql", host, http_port);
    let ws_endpoint = format!("ws://{}:{}/graphql", host, http_port);
    Ok(HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(
            GraphiQLSource::build()
                .endpoint(&graphql_endpoint)
                .subscription_endpoint(&ws_endpoint)
                .finish(),
        ))
}

async fn index_file(req: HttpRequest) -> Result<NamedFile, actix_web::Error> {
    let id = req.match_info().get("id").unwrap();
    println!("{}", id);
    let id = id.split('.').next().unwrap();
    let mut path = PathBuf::new();

    println!("id: {}", id);

    let pool = req.app_data::<Pool<Sqlite>>().unwrap();
    match repo::track::find(pool.clone(), id).await {
        Ok(Some(track)) => {
            path.push(track.path);
            println!("Serving file: {}", path.display());
            let file = NamedFile::open(path)?;
            Ok(file.set_content_disposition(ContentDisposition {
                disposition: DispositionType::Attachment,
                parameters: vec![],
            }))
        }
        Ok(None) => Err(ErrorNotFound("Track not found").into()),
        Err(_) => Err(ErrorNotFound("Track not found").into()),
    }
}

pub async fn start() -> Result<(), Error> {
    let client = reqwest::Client::new();
    let pool = create_connection_pool().await?;
    let playlist_store = PlaylistStore::new(pool.clone());

    let schema = Schema::build(
        Query::default(),
        Mutation::default(),
        Subscription::default(),
    )
    .data(client)
    .data(pool.clone())
    .data(playlist_store)
    .finish();

    let graphql_port = std::env::var("ROCKBOX_GRAPHQL_PORT").unwrap_or("6062".to_string());
    let addr = format!("{}:{}", "0.0.0.0", graphql_port);

    HttpServer::new(move || {
        let home = std::env::var("HOME").unwrap();
        let rockbox_data_dir = format!("{}/.config/rockbox.org", home);
        let covers_path = format!("{}/covers", rockbox_data_dir);
        std::fs::create_dir_all(&covers_path).unwrap();

        let cors =
            Cors::permissive().expose_headers(["Content-Range", "Accept-Ranges", "Content-Length"]);
        App::new()
            .app_data(pool.clone())
            .app_data(Data::new(schema.clone()))
            .wrap(cors)
            .service(index_graphql)
            .service(index_graphiql)
            .service(
                web::resource("/graphql")
                    .guard(guard::Get())
                    .guard(guard::Header("upgrade", "websocket"))
                    .to(index_ws),
            )
            .service(fs::Files::new("/covers", covers_path).show_files_listing())
            .service(index)
            .route("/tracks", web::get().to(index_spa))
            .route("/artists", web::get().to(index_spa))
            .route("/albums", web::get().to(index_spa))
            .route("/files", web::get().to(index_spa))
            .route("/likes", web::get().to(index_spa))
            .route("/settings", web::get().to(index_spa))
            .route("/artists/{_:.*}", web::get().to(index_spa))
            .route("/albums/{_:.*}", web::get().to(index_spa))
            .route("/playlists/{_:.*}", web::get().to(index_spa))
            .route("/files/{_:.*}", web::get().to(index_spa))
            .route("/tracks/{id}", web::get().to(index_file))
            .route("/tracks/{id}", web::head().to(index_file))
            .service(dist)
    })
    .workers(http_workers())
    .bind(addr)?
    .run()
    .await
    .map_err(Error::new)
}
