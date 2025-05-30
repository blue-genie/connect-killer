use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use loco_rs::{prelude::*};
use sea_orm::DeleteResult;
pub use super::_entities::authorized_users::{self, ActiveModel, Entity, Model, Column};

#[derive(Debug, Deserialize, Serialize)]
pub struct AuthorizeParams {
    pub user_id: i32,
    pub device_dongle_id: String,
}

use chrono::prelude::{Utc};
#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    // extend activemodel below (keep comment for generators)
    async fn before_save<C>(self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let mut this = self;
        if insert {
            this.created_at = ActiveValue::Set(Utc::now().naive_utc());
            this.updated_at = ActiveValue::Set(Utc::now().naive_utc());
            Ok(this)
        } else {
            // update time
            this.updated_at = ActiveValue::Set(Utc::now().naive_utc());
            Ok(this)
        }
    }
}


impl super::_entities::authorized_users::Model {
    /// Check if a user is authorized to access a device
    /// 
    /// # Errors
    /// 
    /// Returns true if the user is authorized to access the device, false otherwise
    pub async fn is_user_authorized(
        db: &DatabaseConnection,
        params: &AuthorizeParams
    ) -> bool {
        let permission = authorized_users::Entity::find()
            .filter(Column::UserId.eq(params.user_id))
            .filter(Column::DeviceDongleId.eq(params.device_dongle_id.to_string()))
            .one(db)
            .await
            .expect("Database query failed");

        permission.is_some()
    }

    pub async fn add_authorization(
        db: &DatabaseConnection,
        params: &AuthorizeParams
    ) -> ModelResult<Self> {
        let txn = db.begin().await?;

        let permission = ActiveModel {
            user_id: ActiveValue::Set(params.user_id),
            device_dongle_id: ActiveValue::Set(params.device_dongle_id.to_string()),
            ..Default::default()
        }
        .insert(&txn).await?;
        
        txn.commit().await?;

        Ok(permission)
    }

    pub async fn remove_authorization(
        db: &DatabaseConnection,
        params: &AuthorizeParams,
    ) -> ModelResult<DeleteResult> {
        let txn = db.begin().await?;
    
        // Use DeleteMany for deleting multiple records based on a condition
        let rows = Entity::delete_many()
            .filter(Column::UserId.eq(params.user_id))
            .filter(Column::DeviceDongleId.eq(params.device_dongle_id.to_string()))
            .exec(&txn)
            .await?;
    
        txn.commit().await?;
    
        Ok(rows)
    }

}