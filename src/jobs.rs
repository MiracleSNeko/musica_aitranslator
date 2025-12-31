use anyhow::Result as AnyResult;
use apalis::prelude::{Data, Storage};
use apalis_sql::sqlite::SqliteStorage;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

use crate::storage::{
    TextSegmentColumn, TextSegmentEntity,
    text_segment::{TextSegmentType, create_db_connection},
};

pub trait Job {
    const NAME: &'static str;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserJob {
    pub file_path: PathBuf,
    pub file_name: String,
}

impl Job for ParserJob {
    const NAME: &'static str = "musica-parser-job";
}

pub type ParserJobQueue = SqliteStorage<ParserJob>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblerJob {
    pub file_path: PathBuf,
    pub file_name: String,
}

impl Job for AssemblerJob {
    const NAME: &'static str = "musica-assembler-job";
}

pub type AssemblerJobQueue = SqliteStorage<AssemblerJob>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzerJob {
    pub file_path: PathBuf,
    pub file_name: String,
}

impl Job for AnalyzerJob {
    const NAME: &'static str = "musica-analyzer-job";
}

pub type AnalyzerJobQueue = SqliteStorage<AnalyzerJob>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslatorJob {
    pub file_path: PathBuf,
    pub file_name: String,
}

impl Job for TranslatorJob {
    const NAME: &'static str = "musica-translator-job";
}

pub type TranslatorJobQueue = SqliteStorage<TranslatorJob>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DispatchJob {
    pub file_path: PathBuf,
    pub file_name: String,
}

impl Job for DispatchJob {
    const NAME: &'static str = "musica-dispatch-job";
}

pub type DispatchJobQueue = SqliteStorage<DispatchJob>;

pub async fn dispatch_main(
    job: DispatchJob,
    analyzer: Data<Arc<RwLock<AnalyzerJobQueue>>>,
    translator: Data<Arc<RwLock<TranslatorJobQueue>>>,
) -> AnyResult<()> {
    let (path, name) = (job.file_path, job.file_name);

    {
        let mut analyzer = analyzer.write().await;
        analyzer
            .push(AnalyzerJob {
                file_name: name.clone(),
                file_path: path.clone(),
            })
            .await?;
    }
    {
        let mut translator = translator.write().await;
        translator
            .push(TranslatorJob {
                file_name: name,
                file_path: path,
            })
            .await?;
    }
    Ok(())
}
