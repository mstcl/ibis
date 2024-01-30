use crate::backend::database::schema::edit;
use crate::backend::error::MyResult;
use crate::common::EditVersion;
use crate::common::{DbArticle, DbEdit};
use activitypub_federation::fetch::object_id::ObjectId;
use chrono::{DateTime, Utc};
use diesel::ExpressionMethods;
use diesel::{insert_into, AsChangeset, Insertable, PgConnection, QueryDsl, RunQueryDsl};
use diffy::create_patch;
use std::ops::DerefMut;
use std::sync::Mutex;

#[derive(Debug, Clone, Insertable, AsChangeset)]
#[diesel(table_name = edit, check_for_backend(diesel::pg::Pg))]
pub struct DbEditForm {
    pub creator_id: i32,
    pub hash: EditVersion,
    pub ap_id: ObjectId<DbEdit>,
    pub diff: String,
    pub summary: String,
    pub article_id: i32,
    pub previous_version_id: EditVersion,
    pub created: DateTime<Utc>,
}

impl DbEditForm {
    pub fn new(
        original_article: &DbArticle,
        creator_id: i32,
        updated_text: &str,
        summary: String,
        previous_version_id: EditVersion,
    ) -> MyResult<Self> {
        let diff = create_patch(&original_article.text, updated_text);
        let version = EditVersion::new(&diff.to_string());
        let ap_id = Self::generate_ap_id(original_article, &version)?;
        Ok(DbEditForm {
            hash: version,
            ap_id,
            diff: diff.to_string(),
            creator_id,
            article_id: original_article.id,
            previous_version_id,
            summary,
            created: Utc::now(),
        })
    }

    pub fn generate_ap_id(
        article: &DbArticle,
        version: &EditVersion,
    ) -> MyResult<ObjectId<DbEdit>> {
        Ok(ObjectId::parse(&format!(
            "{}/{}",
            article.ap_id,
            version.hash()
        ))?)
    }
}

impl DbEdit {
    pub fn create(form: &DbEditForm, conn: &Mutex<PgConnection>) -> MyResult<Self> {
        let mut conn = conn.lock().unwrap();
        Ok(insert_into(edit::table)
            .values(form)
            .on_conflict(edit::dsl::ap_id)
            .do_update()
            .set(form)
            .get_result(conn.deref_mut())?)
    }

    pub fn read(version: &EditVersion, conn: &Mutex<PgConnection>) -> MyResult<Self> {
        let mut conn = conn.lock().unwrap();
        Ok(edit::table
            .filter(edit::dsl::hash.eq(version))
            .get_result(conn.deref_mut())?)
    }

    pub fn read_from_ap_id(ap_id: &ObjectId<DbEdit>, conn: &Mutex<PgConnection>) -> MyResult<Self> {
        let mut conn = conn.lock().unwrap();
        Ok(edit::table
            .filter(edit::dsl::ap_id.eq(ap_id))
            .get_result(conn.deref_mut())?)
    }

    pub fn read_for_article(
        article: &DbArticle,
        conn: &Mutex<PgConnection>,
    ) -> MyResult<Vec<Self>> {
        let mut conn = conn.lock().unwrap();
        Ok(edit::table
            .filter(edit::dsl::article_id.eq(article.id))
            .get_results(conn.deref_mut())?)
    }
}
