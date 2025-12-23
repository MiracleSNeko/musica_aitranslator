use crate::{
    jobs::{self, DispatchJob, DispatchJobQueue, ParserJob, ParserJobQueue},
    storage::text_segment::{
        self, InsertModel as TextSegment, InsertModelBuilder as TextSegmentBuilder,
    },
    trustme,
    utils::IntoAnyResult,
};
use anyhow::{Context, Result as AnyResult, bail};
use apalis::prelude::{Attempt, Context as WorkerContext, Data, Storage, TaskId, Worker};
use apalis_sql::{
    context::SqlContext,
    sqlite::{SqlitePool, SqliteStorage},
};
use async_recursion::async_recursion;
use auto_context::auto_context as anyhow_context;
use enum_dispatch::enum_dispatch;
use enum_dispatch_pest_parser::pest_parser;
use pest::{
    Parser,
    iterators::{Pair, Pairs},
};
use sea_orm::{ActiveModelTrait, DatabaseConnection, IntoActiveModel};
use serde_json::{Value as Json, json};
use std::{
    fs::{File, read_to_string},
    io::Write,
    path::PathBuf,
    pin::Pin,
    sync::Arc,
};
use tokio::sync::RwLock;
use walkdir::{DirEntry, WalkDir};

#[pest_parser(grammar = "./src/pest/musica.pest", interface = "MusicaParse")]
pub struct MusicaParser;

#[allow(unused)]
type ParserResult<T> = AnyResult<T>;
#[allow(unused)]
type ParserAst = Pairs<'static, Rule>;
#[allow(unused)]
type ParserAstNode = Pair<'static, Rule>;

#[allow(unused)]
#[enum_dispatch]
pub trait MusicaParse {
    // async_recursion will transform the signature like this:
    // ```rust
    // #[async_recursion(?Send)]
    // async fn function(&self, t: T) -> R;
    //
    // // becomes:
    // #[must_use]
    // fn function<'life_self, 'async_recursion>(
    //      &'life_self self,
    //      t: T
    // ) -> Pin<Box<dyn Future<Output = R> + 'async_recursion>>
    // where
    //     'life_self: 'async_recursion;
    // ```
    fn parse<'life_self, 'async_recursion>(
        &'life_self self,
        node: ParserAstNode,
        line: i32,
        db: Arc<DatabaseConnection>,
    ) -> Pin<Box<dyn Future<Output = ParserResult<Option<TextSegmentBuilder>>> + 'async_recursion>>
    where
        'life_self: 'async_recursion;
}

macro_rules! non_message_node {
    ($t: ty) => {
        impl MusicaParse for $t {
            #[async_recursion(?Send)]
            async fn parse(
                &self,
                node: ParserAstNode,
                line: i32,
                db: Arc<DatabaseConnection>,
            ) -> ParserResult<Option<TextSegmentBuilder>> {
                let model = TextSegmentBuilder::new_non_message()
                    .line(line)
                    .content(node.as_str())
                    .build()?;
                TextSegment::INonMessage(model)
                    .into_active_model()
                    .insert(db.as_ref())
                    .await?;
                Ok(None)
            }
        }
    };
}

macro_rules! silent_node {
    ($t: ty) => {
        impl MusicaParse for $t {
            #[async_recursion(?Send)]
            async fn parse(
                &self,
                _: ParserAstNode,
                _: i32,
                _: Arc<DatabaseConnection>,
            ) -> ParserResult<Option<TextSegmentBuilder>> {
                Ok(None)
            }
        }
    };
}

// silent build-in rules
silent_node!(EOI);
silent_node!(ASCII_PRINTABLE);
silent_node!(CJ_CHARACTERS);
silent_node!(CJ_PUNCTUATION);
silent_node!(CJ_HALF_FULL_WIDTH);
silent_node!(CJ_SEPARATOR);
silent_node!(CJ_LEFT_CORNER_BRACKET);
silent_node!(CJ_RIGHT_CORNER_BRACKET);
silent_node!(CJ_PUNCTUATION_WITHOUT_CORNER_BRACKET);

// silent Musica keywords rules
silent_node!(MUSICA_COMMAND);
silent_node!(MUSICA_PREPROC);
silent_node!(MUSICA_COMMENT);

// silent Musica rules
silent_node!(IMusicaScript);

// ;comment rule
non_message_node!(IComment);

// #include rule
non_message_node!(IInclude);

// .message rule
impl MusicaParse for IMessage {
    #[async_recursion(?Send)]
    async fn parse(
        &self,
        node: ParserAstNode,
        line: i32,
        db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        // IMessage ONLY contains ONE IMessageNamed or IMessageUnnamed
        if let Some(node) = node.into_inner().next() {
            let rule = node.as_rule();
            let builder = rule
                .parse(node, line, db.clone())
                .await?
                .into_any_result()?;
            if let TextSegmentBuilder::IMessage(builder) = builder {
                let message = builder.build()?;
                TextSegment::IMessage(message)
                    .into_active_model()
                    .insert(db.as_ref())
                    .await?;
            } else {
                bail!("Expected IMessageBuilder, found INonMessageBuilder");
            }
        }
        Ok(None)
    }
}

impl MusicaParse for IMessageNamed {
    #[async_recursion(?Send)]
    async fn parse<'a>(
        &self,
        node: ParserAstNode,
        line: i32,
        db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        let mut builder = TextSegmentBuilder::new_message().line(line);
        for node in node.into_inner() {
            let rule = node.as_rule();
            let segment = rule
                .parse(node, line, db.clone())
                .await?
                .into_any_result()?;
            builder = builder.combine(segment)?;
        }

        Ok(Some(builder.into()))
    }
}

impl MusicaParse for IMessageUnnamed {
    #[async_recursion(?Send)]
    async fn parse(
        &self,
        node: ParserAstNode,
        line: i32,
        db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        let mut builder = TextSegmentBuilder::new_message().line(line);
        for node in node.into_inner() {
            let rule = node.as_rule();
            let segment = rule
                .parse(node, line, db.clone())
                .await?
                .into_any_result()?;
            builder = builder.combine(segment)?;
        }

        Ok(Some(builder.into()))
    }
}

// .message atoms
impl MusicaParse for MessageNumber {
    #[async_recursion(?Send)]
    async fn parse(
        &self,
        node: ParserAstNode,
        _line: i32,
        _db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        Ok(Some(
            TextSegmentBuilder::new_message()
                .id(node.as_str().parse::<i32>()?)
                .into(),
        ))
    }
}

impl MusicaParse for MessageSpeakerName {
    #[async_recursion(?Send)]
    async fn parse(
        &self,
        node: ParserAstNode,
        _line: i32,
        _db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        Ok(Some(
            TextSegmentBuilder::new_message().name(node.as_str()).into(),
        ))
    }
}

impl MusicaParse for MessageSpeakerTachie {
    #[async_recursion(?Send)]
    async fn parse(
        &self,
        node: ParserAstNode,
        _line: i32,
        _db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        Ok(Some(
            TextSegmentBuilder::new_message()
                .tachie(node.as_str())
                .into(),
        ))
    }
}

impl MusicaParse for MessageContentQuoted {
    #[async_recursion(?Send)]
    async fn parse(
        &self,
        node: ParserAstNode,
        _line: i32,
        _db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        Ok(Some(
            TextSegmentBuilder::new_message()
                .content(node.as_str())
                .into(),
        ))
    }
}

impl MusicaParse for MessageContentUnquoted {
    #[async_recursion(?Send)]
    async fn parse(
        &self,
        node: ParserAstNode,
        _line: i32,
        _db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        Ok(Some(
            TextSegmentBuilder::new_message()
                .content(node.as_str())
                .into(),
        ))
    }
}

// non .message rule for text extraction
non_message_node!(INonMessage);

// main rule for Musica
impl MusicaParse for Musica {
    #[async_recursion(?Send)]
    async fn parse(
        &self,
        node: ParserAstNode,
        line: i32,
        db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        let mut line = line;
        for node in node.into_inner() {
            let rule = node.as_rule();
            let _ = rule
                .parse(node, line, db.clone())
                .await?
                .into_any_result()?;
            line += 1;
        }
        Ok(None)
    }
}

#[anyhow_context]
pub async fn parse_file(path: PathBuf, name: String) -> ParserResult<()> {
    let db = text_segment::create_db_connection(&name).await?;

    // SAFETY: `content` will be used only once inside this function scope
    let content = unsafe { trustme::ScopedStaticStr::new(read_to_string(path)?) };

    let ast: ParserAst = MusicaParser::parse(Rule::Musica(Musica {}), content.as_static_str())?;
    let root: ParserAstNode = ast.peek().into_any_result()?;
    let rule = root.as_rule();

    rule.parse(root, 0, db).await?;

    // SAFETY: `content` will be dropped here and never used after drop
    Ok(())
}

pub async fn parser_main(
    job: ParserJob,
    dispatch: Data<Arc<RwLock<DispatchJobQueue>>>,
) -> AnyResult<()> {
    parse_file(job.file_path.clone(), job.file_name.clone()).await?;
    let mut dispatch = dispatch.write().await;
    dispatch
        .push(DispatchJob {
            file_name: job.file_name,
            file_path: job.file_path,
        })
        .await?;
    Ok(())
}
