use crate::database::DatabaseHandle;
use crate::error::MyResult;
use crate::federation::activities::create_article::CreateArticle;
use crate::federation::activities::update_article::UpdateArticle;
use crate::federation::objects::article::DbArticle;
use crate::federation::objects::edit::{DbEdit, EditVersion};
use crate::federation::objects::instance::DbInstance;
use activitypub_federation::config::Data;
use activitypub_federation::fetch::object_id::ObjectId;

use anyhow::anyhow;

use axum::extract::Query;
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use axum_macros::debug_handler;
use serde::{Deserialize, Serialize};
use url::Url;

pub fn api_routes() -> Router {
    Router::new()
        .route(
            "/article",
            get(get_article).post(create_article).patch(edit_article),
        )
        .route("/resolve_instance", get(resolve_instance))
        .route("/resolve_article", get(resolve_article))
        .route("/instance", get(get_local_instance))
        .route("/instance/follow", post(follow_instance))
}

#[derive(Deserialize, Serialize)]
pub struct CreateArticleData {
    pub title: String,
}

#[debug_handler]
async fn create_article(
    data: Data<DatabaseHandle>,
    Form(create_article): Form<CreateArticleData>,
) -> MyResult<Json<DbArticle>> {
    let local_instance_id = data.local_instance().ap_id;
    let ap_id = ObjectId::parse(&format!(
        "http://{}:{}/article/{}",
        local_instance_id.inner().domain().unwrap(),
        local_instance_id.inner().port().unwrap(),
        create_article.title
    ))?;
    let article = DbArticle {
        title: create_article.title,
        text: String::new(),
        ap_id,
        latest_version: EditVersion::default(),
        edits: vec![],
        instance: local_instance_id,
        local: true,
    };
    {
        let mut articles = data.articles.lock().unwrap();
        articles.insert(article.ap_id.inner().clone(), article.clone());
    }

    CreateArticle::send_to_followers(article.clone(), &data).await?;

    Ok(Json(article))
}

#[derive(Deserialize, Serialize, Debug)]
pub struct EditArticleData {
    pub ap_id: ObjectId<DbArticle>,
    pub new_text: String,
}

#[debug_handler]
async fn edit_article(
    data: Data<DatabaseHandle>,
    Form(edit_article): Form<EditArticleData>,
) -> MyResult<()> {
    let original_article = {
        let mut lock = data.articles.lock().unwrap();
        let article = lock.get_mut(edit_article.ap_id.inner()).unwrap();
        article.clone()
    };
    let edit = DbEdit::new(&original_article, &edit_article.new_text)?;
    if original_article.local {
        let updated_article = {
            let mut lock = data.articles.lock().unwrap();
            let article = lock.get_mut(edit_article.ap_id.inner()).unwrap();
            article.text = edit_article.new_text;
            article.latest_version = edit.version.clone();
            article.edits.push(edit.clone());
            article.clone()
        };

        UpdateArticle::send_to_followers(edit, updated_article.clone(), &data).await?;
    } else {
        UpdateArticle::send_to_origin(
            edit,
            // TODO: should be dereference(), but then article is refetched which breaks test_edit_conflict()
            original_article.instance.dereference_local(&data).await?,
            &data,
        )
        .await?;
    }

    Ok(())
}

#[derive(Deserialize, Serialize, Clone)]
pub struct GetArticleData {
    pub title: String,
}

#[debug_handler]
async fn get_article(
    Query(query): Query<GetArticleData>,
    data: Data<DatabaseHandle>,
) -> MyResult<Json<DbArticle>> {
    let articles = data.articles.lock().unwrap();
    let article = articles
        .iter()
        .find(|a| a.1.title == query.title)
        .ok_or(anyhow!("not found"))?
        .1
        .clone();
    Ok(Json(article))
}

#[derive(Deserialize, Serialize)]
pub struct ResolveObject {
    pub id: Url,
}

#[debug_handler]
async fn resolve_instance(
    Query(query): Query<ResolveObject>,
    data: Data<DatabaseHandle>,
) -> MyResult<Json<DbInstance>> {
    let instance: DbInstance = ObjectId::from(query.id).dereference(&data).await?;
    Ok(Json(instance))
}

#[debug_handler]
async fn resolve_article(
    Query(query): Query<ResolveObject>,
    data: Data<DatabaseHandle>,
) -> MyResult<Json<DbArticle>> {
    let article: DbArticle = ObjectId::from(query.id).dereference(&data).await?;
    Ok(Json(article))
}

#[debug_handler]
async fn get_local_instance(data: Data<DatabaseHandle>) -> MyResult<Json<DbInstance>> {
    Ok(Json(data.local_instance()))
}

#[derive(Deserialize, Serialize, Debug)]
pub struct FollowInstance {
    pub instance_id: ObjectId<DbInstance>,
}

#[debug_handler]
async fn follow_instance(
    data: Data<DatabaseHandle>,
    Form(query): Form<FollowInstance>,
) -> MyResult<()> {
    let instance = query.instance_id.dereference(&data).await?;
    data.local_instance().follow(&instance, &data).await?;
    Ok(())
}
