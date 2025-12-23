use anyhow::Result as AnyResult;
use apalis::{
    layers::WorkerBuilderExt,
    prelude::{Monitor, Storage, WorkerBuilder, WorkerFactory, WorkerFactoryFn},
};
use apalis_sql::sqlite::{SqlitePool, SqliteStorage};
use lazy_static::lazy_static;
use sea_orm::DatabaseConnection;
use std::sync::Arc;
use tokio::sync::RwLock;
use walkdir::WalkDir;

mod analyzer;
mod assembler;
mod jobs;
mod parser;
mod storage;
mod translator;
mod trustme;
mod utils;

use crate::{
    jobs::{
        AnalyzerJobQueue, AssemblerJobQueue, DispatchJob, DispatchJobQueue, Job, ParserJob,
        ParserJobQueue, TranslatorJobQueue,
    },
    parser::*,
    storage::text_segment::create_db_connection,
    utils::IntoAnyResult,
};

lazy_static! {
    // To push connections to the keep-alive list in an `async` context,
    // we need to use an extra `RwLock` to allow concurrent access.
    static ref KEEP_ALIVE: Arc<RwLock<Vec<Arc<DatabaseConnection>>>> =
        Arc::new(RwLock::new(Vec::new()));
}

#[tokio::main]
async fn main() -> AnyResult<()> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    SqliteStorage::setup(&pool).await?;

    let mut parser_jobs = ParserJobQueue::new(pool.clone());
    let assembler_jobs = AssemblerJobQueue::new(pool.clone());
    let analyzer_jobs = AnalyzerJobQueue::new(pool.clone());
    let translator_jobs = TranslatorJobQueue::new(pool.clone());
    let dispatch_jobs = DispatchJobQueue::new(pool.clone());

    let mut keep_alive = KEEP_ALIVE.write().await;
    for entry in WalkDir::new("./assets/sc")
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        let job = ParserJob {
            file_path: entry.path().to_path_buf(),
            file_name: entry.file_name().to_string_lossy().to_string(),
        };
        keep_alive.push(create_db_connection(&job.file_name).await?);
        parser_jobs.push(job).await?;
    }

    let monitor = Monitor::new()
        .register({
            WorkerBuilder::new(ParserJob::NAME)
                .data(Arc::new(RwLock::new(dispatch_jobs.clone())))
                .concurrency(4)
                .backend(parser_jobs)
                .build_fn(parser_main)
        })
        .register({
            WorkerBuilder::new(DispatchJob::NAME)
                .concurrency(2)
                .backend(dispatch_jobs)
                .build_fn(|_| async move {})
        });

    Ok(())
}
