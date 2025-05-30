#![allow(clippy::unused_async)]
use loco_rs::prelude::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use axum::{
    extract::{Query, State}, Extension,
};
extern crate url;
use std::{collections::HashMap, env};

use crate::{models::_entities, views};

#[derive(Deserialize)]
pub struct OneBox {
    onebox: Option<String>
}

#[derive(Serialize)]
pub struct UsersTemplate {
    pub defined: bool,
    pub users: Vec<_entities::users::Model>
}
#[derive(Serialize)]
pub struct RoutesTemplate {
    pub defined: bool,
    pub routes: Vec<_entities::routes::Model>
}
#[derive(Serialize)]
pub struct DevicesTemplate {
    pub defined: bool,
    pub devices: Vec<_entities::devices::Model>
}
#[derive(Serialize)]
pub struct BootlogsTemplate {
    pub defined: bool,
    pub bootlogs: Vec<_entities::bootlogs::Model>
}

#[derive(Serialize)]
pub struct CloudlogsTemplate {
    pub defined: bool,
    // Nested summary: branch -> (map: module -> count of logs)
    pub cloudlogs: std::collections::HashMap<String, std::collections::HashMap<String, usize>>,
}

#[derive(Serialize)]
pub struct SegmentsTemplate {
    pub defined: bool,
    pub segments: Vec<_entities::segments::Model>,
}

#[derive(Serialize, Default)]
pub struct MasterTemplate {
    pub api_host: String,
    pub ws_host: String,
    pub onebox: String,
    pub dongle_id: String,
    pub users: Option<UsersTemplate>,
    pub segments: Option<SegmentsTemplate>,
    pub devices: Option<DevicesTemplate>,
    pub routes: Option<RoutesTemplate>,
    pub bootlogs: Option<BootlogsTemplate>,
    pub cloudlogs: Option<CloudlogsTemplate>,
}

pub async fn onebox_handler(
    auth: crate::middleware::auth::MyJWT,
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
    Query(params): Query<OneBox>,
    Extension(manager): Extension<std::sync::Arc<super::ws::ConnectionManager>>,
) -> Result<impl IntoResponse> {
    
    let user_model = match auth.user_model {
        Some(user_model) => user_model,
        None => return unauthorized("Couldn't find user"), // Error handling for when auth.user_model is None. Should never get here
    };
    let onebox = match params.onebox {
        Some(onebox) => onebox,
        None => user_model.name.clone(),
    };
    use crate::common::re::*;
    let route_match_string = format!(
        r"^({DONGLE_ID})([_|/|]?({ROUTE_NAME}))?"
    );
    // Regex to match a complete canonical route name
    let re = regex::Regex::new(&route_match_string).unwrap();

    let mut canonical_route_name: Option<String> = None;
    let mut dongle_id: Option<String> = None;
    let mut timestamp: Option<String> = None;


    // Check for route or dongle ID
    if let Some(caps) = re.captures(&onebox) {
        dongle_id = Some(caps[1].to_string());
        if let Some(ts) = caps.get(3) {
            timestamp = Some(ts.as_str().to_string());
            canonical_route_name = Some(format!("{}|{}", dongle_id.as_ref().unwrap(), timestamp.as_ref().unwrap()));
        }
    }
    let api_endpoint: String = env::var("API_ENDPOINT").expect("API_ENDPOINT env variable not set");
    let ws_endpoint: String = env::var("WS_ENDPOINT").expect("WS_ENDPOINT env variable not set");

    let mut master_template = MasterTemplate {
        dongle_id: dongle_id.clone().unwrap_or_default(),
        onebox: onebox,
        api_host: api_endpoint,
        ws_host: ws_endpoint,
        ..Default::default()
    };
    if user_model.superuser {
        master_template.users = Some(UsersTemplate {
            defined: true,
            users: _entities::users::Model::find_all_users(&ctx.db).await
        });
    } else {
        master_template.users = Some(UsersTemplate {
            defined: true,
            users: vec![user_model.clone()],
        });
    }

    // Parse cloudlogs from the connection manager's cache and add them to the template if available.
    if let Some(ref d_id) = dongle_id {
        // Retrieve the nested logs from the cloudlog cache.
        // Here we assume the cache is stored as:
        // HashMap<String, HashMap<String, HashMap<String, Vec<Value>>>>
        // where the outer key is the device id, then branch, then module.
        let nested_logs: Option<HashMap<String, HashMap<String, Vec<Value>>>> = {
            let cloudlog_cache = manager.cloudlog_cache.read().await;
            cloudlog_cache.get(d_id).cloned()
        };
    
        // Build a summary: for each branch and module, count the number of logs.
        let summary: HashMap<String, HashMap<String, usize>> =
            if let Some(nested) = nested_logs {
                let mut map = HashMap::new();
                for (branch, modules) in nested {
                    let mut module_map = HashMap::new();
                    for (module, logs_array) in modules {
                        module_map.insert(module, logs_array.len());
                    }
                    map.insert(branch, module_map);
                }
                map
            } else {
                HashMap::new()
            };
    
        master_template.cloudlogs = Some(CloudlogsTemplate {
            defined: !summary.is_empty(),
            cloudlogs: summary,
        });
    }

    if let Some(canonical_route) = canonical_route_name {
        let mut segment_models = Some(_entities::segments::Model::find_segments_by_route(&ctx.db, &canonical_route).await?);
        if let Some(segment_models) = segment_models.as_mut() {
            segment_models.sort_by(|a, b| a.number.cmp(&b.number));
        }

        master_template.segments = segment_models.map(|segments| SegmentsTemplate { 
            defined: true, 
            segments 
        });
    
        views::route::admin_route(v, master_template)
    } else if let Some(d_id) = dongle_id {
        master_template.routes = Some(RoutesTemplate { 
            defined: true, 
            routes: _entities::routes::Model::find_device_routes(&ctx.db, &d_id).await?, 
        });
        master_template.devices = Some(DevicesTemplate {
            defined: true,
            devices: _entities::devices::Model::find_user_devices(&ctx.db, user_model.id).await,
        });
        master_template.bootlogs = Some(BootlogsTemplate {
            defined: true,
            bootlogs: _entities::bootlogs::Model::find_device_bootlogs(&ctx.db, &d_id).await?,
        });
        views::route::admin_route(v, master_template)

    } else {
        if user_model.superuser {
            master_template.devices = Some(DevicesTemplate {
                defined: true,
                devices: _entities::devices::Model::find_all_devices(&ctx.db).await
            });
        } else {
            master_template.devices = Some(DevicesTemplate {
                defined: true,
                devices: _entities::devices::Model::find_user_devices(&ctx.db, user_model.id).await
            });

        };
        // Fallback response
        views::route::admin_route(v, master_template)
    }
}

pub async fn login(
    ViewEngine(v): ViewEngine<TeraView>,
    State(ctx): State<AppContext>,
) -> Result<impl IntoResponse> {
    views::auth::login(
        v, 
        crate::views::auth::LoginTemplate { 
            api_host: env::var("API_ENDPOINT").expect("API_ENDPOINT env variable not set")
        }
    )
}

pub fn routes() -> Routes {
    Routes::new()
        .add("/", get(onebox_handler))
        .add("/login", get(login))
}
