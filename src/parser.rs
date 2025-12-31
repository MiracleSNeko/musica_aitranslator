use crate::{
    jobs::{DispatchJob, DispatchJobQueue, ParserJob},
    storage::{create_db_connection, create_table, TextSegment, TextSegmentBuilder},
    utils::IntoAnyResult,
};
use anyhow::{Context, Result as AnyResult, bail};
use apalis::prelude::{Data, Storage};
use auto_context::auto_context as anyhow_context;
use enum_dispatch::enum_dispatch;
use enum_dispatch_pest_parser::pest_parser;
use futures::executor::block_on;
use pest::{
    Parser,
    iterators::{Pair, Pairs},
};
use sea_orm::{ActiveModelTrait, DatabaseConnection, IntoActiveModel};
use std::{fs::read_to_string, path::PathBuf, sync::Arc};
use tokio::sync::RwLock;

#[pest_parser(grammar = "./src/pest/musica.pest", interface = "MusicaParse")]
pub struct MusicaParser;

#[allow(unused)]
type ParserResult<T> = AnyResult<T>;
#[allow(unused)]
type ParserAst<'a> = Pairs<'a, Rule>;
#[allow(unused)]
type ParserAstNode<'a> = Pair<'a, Rule>;
#[allow(unused)]
type StaticParserAst = Pairs<'static, Rule>;
#[allow(unused)]
type StaticParserAstNode = Pair<'static, Rule>;

#[allow(unused)]
#[enum_dispatch]
pub trait MusicaParse {
    fn parse(
        &self,
        node: ParserAstNode,
        line: i32,
        db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>>;
}

macro_rules! non_message_node {
    ($t: ty) => {
        impl MusicaParse for $t {
            fn parse(
                &self,
                node: ParserAstNode,
                line: i32,
                db: Arc<DatabaseConnection>,
            ) -> ParserResult<Option<TextSegmentBuilder>> {
                let model = TextSegmentBuilder::new_non_message()
                    .line(line)
                    .content(node.as_str())
                    .build()?;
                block_on(
                    TextSegment::INonMessage(model)
                        .into_active_model()
                        .insert(db.as_ref()),
                )?;
                Ok(None)
            }
        }
    };
}

macro_rules! silent_node {
    ($t: ty) => {
        impl MusicaParse for $t {
            fn parse(
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
    fn parse(
        &self,
        node: ParserAstNode,
        line: i32,
        db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        // IMessage ONLY contains ONE IMessageNamed or IMessageUnnamed
        if let Some(node) = node.into_inner().next() {
            let rule = node.as_rule();
            let builder = rule.parse(node, line, db.clone())?.into_any_result()?;
            if let TextSegmentBuilder::IMessage(builder) = builder {
                let message = builder.build()?;
                block_on(
                    TextSegment::IMessage(message)
                        .into_active_model()
                        .insert(db.as_ref()),
                )?;
            } else {
                bail!("Expected IMessageBuilder, found INonMessageBuilder");
            }
        }
        Ok(None)
    }
}

impl MusicaParse for IMessageNamed {
    fn parse(
        &self,
        node: ParserAstNode,
        line: i32,
        db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        let mut builder = TextSegmentBuilder::new_message().line(line);
        for node in node.into_inner() {
            let rule = node.as_rule();
            let segment = rule.parse(node, line, db.clone())?.into_any_result()?;
            builder = builder.combine(segment)?;
        }

        Ok(Some(builder.into()))
    }
}

impl MusicaParse for IMessageUnnamed {
    fn parse(
        &self,
        node: ParserAstNode,
        line: i32,
        db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        let mut builder = TextSegmentBuilder::new_message().line(line);
        for node in node.into_inner() {
            let rule = node.as_rule();
            let segment = rule.parse(node, line, db.clone())?.into_any_result()?;
            builder = builder.combine(segment)?;
        }

        Ok(Some(builder.into()))
    }
}

// .message atoms
impl MusicaParse for MessageNumber {
    fn parse(
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
    fn parse(
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
    fn parse(
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
    fn parse(
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
    fn parse(
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
    fn parse(
        &self,
        node: ParserAstNode,
        line: i32,
        db: Arc<DatabaseConnection>,
    ) -> ParserResult<Option<TextSegmentBuilder>> {
        let mut line = line;
        for node in node.into_inner() {
            let rule = node.as_rule();
            let _ = rule.parse(node, line, db.clone())?.into_any_result()?;
            line += 1;
        }
        Ok(None)
    }
}
#[anyhow_context]
pub fn parse_file(path: PathBuf, name: String) -> ParserResult<()> {
    let db = block_on(create_db_connection(&name))?;
    block_on(create_table(db.clone()))?;

    let content = read_to_string(path)?;
    let ast: ParserAst = MusicaParser::parse(Rule::Musica(Musica {}), &content)?;
    let root: ParserAstNode = ast.peek().into_any_result()?;
    let rule = root.as_rule();

    rule.parse(root, 0, db)?;
    Ok(())
}

pub async fn parser_main(
    job: ParserJob,
    dispatch: Data<Arc<RwLock<DispatchJobQueue>>>,
) -> AnyResult<()> {
    let (path, name) = (job.file_path, job.file_name);
    parse_file(path.clone(), name.clone())?;
    let mut dispatch = dispatch.write().await;
    dispatch
        .push(DispatchJob {
            file_name: name,
            file_path: path,
        })
        .await?;
    Ok(())
}
