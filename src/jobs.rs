use apalis_sql::sqlite::SqliteStorage;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
