pub mod text_segment {
    use anyhow::{Context, Result as AnyResult, bail};
    use auto_context::auto_context as anyhow_context;
    use derive_builder::Builder;
    use sea_orm::{
        ActiveValue::Set, ConnectionTrait, Database, DatabaseConnection, IntoActiveModel, Schema,
        entity::prelude::*,
    };
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::sync::Arc;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "text_segments")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        #[sea_orm(enum_name = "text_segment_type")]
        pub segment_type: TextSegmentType,
        #[sea_orm(column_type = "JsonBinary")]
        pub content: Json,
    }

    #[derive(
        Copy, Clone, Debug, PartialEq, Eq, EnumIter, Serialize, Deserialize, DeriveActiveEnum,
    )]
    #[sea_orm(rs_type = "i32", db_type = "Integer")]
    pub enum TextSegmentType {
        IMessage = 0,
        INonMessage = 1,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}

    #[derive(Builder, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[builder(pattern = "owned")]
    pub struct IMessageModel {
        #[builder(setter(into))]
        pub line: i32,
        #[builder(setter(into))]
        pub id: i32,
        #[builder(setter(into), default = String::new())]
        pub name: String,
        #[builder(setter(into), default = String::new())]
        pub tachie: String,
        #[builder(setter(into))]
        pub content: String,
    }

    #[derive(Builder, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[builder(pattern = "owned")]
    pub struct INonMessageModel {
        #[builder(setter(into))]
        pub line: i32,
        #[builder(setter(into))]
        pub content: String,
    }

    #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "type")]
    pub enum InsertModel {
        IMessage(IMessageModel),
        INonMessage(INonMessageModel),
    }

    impl Into<InsertModel> for IMessageModel {
        fn into(self) -> InsertModel {
            InsertModel::IMessage(self)
        }
    }

    impl Into<InsertModel> for INonMessageModel {
        fn into(self) -> InsertModel {
            InsertModel::INonMessage(self)
        }
    }

    impl From<InsertModel> for ActiveModel {
        fn from(insert_model: InsertModel) -> Self {
            let content = json!(insert_model);
            ActiveModel {
                segment_type: match insert_model {
                    InsertModel::IMessage(_) => Set(TextSegmentType::IMessage),
                    InsertModel::INonMessage(_) => Set(TextSegmentType::INonMessage),
                },
                content: Set(content),
                ..Default::default()
            }
        }
    }

    impl IntoActiveModel<ActiveModel> for InsertModel {
        fn into_active_model(self) -> ActiveModel {
            let content = json!(self);
            ActiveModel {
                segment_type: match self {
                    InsertModel::IMessage(_) => Set(TextSegmentType::IMessage),
                    InsertModel::INonMessage(_) => Set(TextSegmentType::INonMessage),
                },
                content: Set(content),
                ..Default::default()
            }
        }
    }

    impl IMessageModelBuilder {
        pub fn combine(self, other: InsertModelBuilder) -> AnyResult<Self> {
            fn merge_exclusive<T>(a: Option<T>, b: Option<T>, filed: &str) -> AnyResult<Option<T>> {
                match (a, b) {
                    (Some(_), Some(_)) => bail!("Conflict when merging field `{}`", filed),
                    (Some(v), None) | (None, Some(v)) => Ok(Some(v)),
                    (None, None) => Ok(None),
                }
            }
            match other {
                InsertModelBuilder::INonMessage(_) => {
                    bail!("Cannot combine IMessageModelBuilder with INonMessageModelBuilder")
                }
                InsertModelBuilder::IMessage(other) => Ok(IMessageModelBuilder {
                    line: merge_exclusive(self.line, other.line, "line")?,
                    id: merge_exclusive(self.id, other.id, "id")?,
                    name: merge_exclusive(self.name, other.name, "name")?,
                    tachie: merge_exclusive(self.tachie, other.tachie, "tachie")?,
                    content: merge_exclusive(self.content, other.content, "content")?,
                    ..Default::default()
                }),
            }
        }
    }

    impl Into<InsertModelBuilder> for IMessageModelBuilder {
        fn into(self) -> InsertModelBuilder {
            InsertModelBuilder::IMessage(self)
        }
    }

    impl INonMessageModelBuilder {
        pub fn combine(self, other: InsertModelBuilder) -> AnyResult<Self> {
            fn merge_exclusive<T>(a: Option<T>, b: Option<T>, filed: &str) -> AnyResult<Option<T>> {
                match (a, b) {
                    (Some(_), Some(_)) => bail!("Conflict when merging field `{}`", filed),
                    (Some(v), None) | (None, Some(v)) => Ok(Some(v)),
                    (None, None) => Ok(None),
                }
            }
            match other {
                InsertModelBuilder::IMessage(_) => {
                    bail!("Cannot combine INonMessageModelBuilder with IMessageModelBuilder")
                }
                InsertModelBuilder::INonMessage(other) => Ok(INonMessageModelBuilder {
                    line: merge_exclusive(self.line, other.line, "line")?,
                    content: merge_exclusive(self.content, other.content, "content")?,
                    ..Default::default()
                }),
            }
        }
    }

    impl Into<InsertModelBuilder> for INonMessageModelBuilder {
        fn into(self) -> InsertModelBuilder {
            InsertModelBuilder::INonMessage(self)
        }
    }

    pub enum InsertModelBuilder {
        IMessage(IMessageModelBuilder),
        INonMessage(INonMessageModelBuilder),
    }

    impl InsertModelBuilder {
        pub fn new_message() -> IMessageModelBuilder {
            IMessageModelBuilder::default()
        }

        pub fn new_non_message() -> INonMessageModelBuilder {
            INonMessageModelBuilder::default()
        }

        pub fn combine(self, other: Self) -> AnyResult<Self> {
            match self {
                InsertModelBuilder::IMessage(builder) => {
                    let combined = builder.combine(other)?;
                    Ok(combined.into())
                }
                InsertModelBuilder::INonMessage(builder) => {
                    let combined = builder.combine(other)?;
                    Ok(combined.into())
                }
            }
        }
    }

    #[anyhow_context]
    pub async fn create_db_connection(name: &str) -> AnyResult<Arc<DatabaseConnection>> {
        let url = format!("file:{name}?mode=memory&cache=shared");
        let db = Database::connect(url).await?;
        Ok(Arc::new(db))
    }

    #[anyhow_context]
    pub async fn create_table(db: Arc<DatabaseConnection>) -> AnyResult<()> {
        let backend = db.get_database_backend();
        let schema = Schema::new(backend);

        let statement = backend.build(schema.create_table_from_entity(Entity).if_not_exists());
        db.execute(statement).await?;
        Ok(())
    }
}

pub use text_segment::{
    Column as TextSegmentColumn, Entity as TextSegmentEntity, InsertModel as TextSegment,
    InsertModelBuilder as TextSegmentBuilder, create_db_connection, create_table,
};
