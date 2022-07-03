use itertools::Itertools;

pub enum Operation {
    Add,
    Delete,
    Modify,
}

#[derive(serde::Deserialize, serde::Serialize, std::fmt::Debug, Clone)]
pub struct EditRequest {
    pub url: String,
    pub value: Option<String>,
    pub path: String,
    pub operation: Option<String>, // todo: convert it to enum
    pub data: Option<String>,
    pub root: Option<String>,
}

impl EditRequest {
    pub(crate) fn is_delete(&self) -> bool {
        matches!(self.operation.as_ref(), Some(v) if v.eq("delete"))
    }

    pub(crate) fn is_rename(&self) -> bool {
        matches!(self.operation.as_ref(), Some(v) if v.eq("rename"))
    }

    pub(crate) fn set_delete(mut self) -> EditRequest {
        self.operation = Some("delete".to_string());
        self
    }

    pub(crate) fn set_add(mut self) -> EditRequest {
        self.operation = Some("add".to_string());
        self
    }
}

#[derive(serde::Serialize, serde::Deserialize, std::fmt::Debug)]
pub struct EditResponse {
    pub path: String,
    pub url: Option<String>,
    pub reload: bool,
}

pub async fn edit(
    req: actix_web::web::Json<EditRequest>,
) -> actix_web::Result<actix_web::HttpResponse> {
    match edit_worker(req.0).await {
        Ok(data) => fpm::apis::success(data),
        Err(err) => fpm::apis::error(
            err.to_string(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        ),
    }
}

pub(crate) async fn edit_worker(request: EditRequest) -> fpm::Result<EditResponse> {
    let mut config = fpm::Config::read2(None, false).await?;

    if let Some((cr_number, cr_path)) =
        fpm::cr::get_cr_and_path_from_id(&request.path, &request.root)
    {
        if request.is_rename() {
            let rename = match request.data {
                Some(ref v) if !v.is_empty() => v,
                _ => {
                    return Err(fpm::Error::APIResponseError(
                        "rename value should present".to_string(),
                    ));
                }
            };
            // Delete previous path
            {
                let request = request.clone().set_delete();
                handle_cr_edit(&mut config, &request, cr_number, cr_path.as_str()).await?;
            }
            // Add new path
            let request = request.clone().set_add();
            return handle_cr_edit(&mut config, &request, cr_number, rename.as_str()).await;
        }
        return handle_cr_edit(&mut config, &request, cr_number, cr_path.as_str()).await;
    }

    if request.is_delete() {
        let path = config.root.join(&request.path);
        if path.is_dir() {
            tokio::fs::remove_dir_all(&path).await?;
        } else if path.is_file() {
            tokio::fs::remove_file(&path).await?;
        }
        return Ok(EditResponse {
            path: request.path,
            url: None,
            reload: true,
        });
    }

    if request.is_rename() {
        let rename = match request.data {
            Some(ref v) if !v.is_empty() => v.to_string(),
            _ => {
                return Err(fpm::Error::APIResponseError(
                    "rename value should present".to_string(),
                ));
            }
        };

        let new_path = if let Some((p, _)) = request.path.trim_end_matches('/').rsplit_once("/") {
            format!("{}/{}", p, rename)
        } else {
            rename
        };

        tokio::fs::rename(config.root.join(&request.path), config.root.join(new_path)).await?;

        // TODO: redirect to renamed file, if folder so it will redirect to renamed folder with
        // index.ftd, if index.ftd does not exists so it will redirected to main project index.ftd

        return Ok(EditResponse {
            path: request.path,
            url: None,
            reload: true,
        });
    }

    // Handle Modify and Add
    handle_add_modify(&mut config, request.path.as_str(), None, request.value).await
}

async fn handle_add_modify(
    config: &mut fpm::Config,
    path: &str,
    root: Option<String>,
    value: Option<String>,
) -> fpm::Result<EditResponse> {
    let (file_name, url, before_update_status) = if let Ok((path, has_root)) = config
        .get_file_path_with_root(path, root.clone(), Default::default())
        .await
    {
        let snapshots = fpm::snapshot::get_latest_snapshots(&config.root).await?;
        let workspaces = fpm::snapshot::get_workspace(config).await?;

        let file = fpm::get_file(
            config.package.name.to_string(),
            &config.root.join(&path),
            &(match root {
                Some(ref root) if has_root => config.root.join(root),
                _ => config.root.clone(),
            }),
        )
        .await?;
        let before_update_status =
            fpm::commands::status::get_file_status(config, &file, &snapshots, &workspaces).await?;

        (
            fpm::utils::path_with_root(path.as_str(), &root),
            None,
            Some(before_update_status),
        )
    } else if path.ends_with('/') {
        let path = format!("{}index.ftd", path);
        (
            fpm::utils::path_with_root(path.as_str(), &root),
            Some(format!(
                "-/view-src/{}",
                fpm::utils::path_with_root(path.as_str(), &root).trim_start_matches('/')
            )),
            None,
        )
    } else {
        (
            fpm::utils::path_with_root(path, &root),
            Some(format!(
                "-/view-src/{}",
                fpm::utils::path_with_root(path, &root).trim_start_matches('/')
            )),
            None,
        )
    };

    fpm::utils::update(
        &config.root,
        file_name.as_str(),
        value
            .unwrap_or_else(|| "".to_string())
            .into_bytes()
            .as_slice(),
    )
    .await?;

    if let Some(before_update_status) = before_update_status {
        let snapshots = fpm::snapshot::get_latest_snapshots(&config.root).await?;
        let workspaces = fpm::snapshot::get_workspace(config).await?;
        let file = fpm::get_file(
            config.package.name.to_string(),
            &config.root.join(&file_name),
            &(match root {
                Some(ref root) => config.root.join(root),
                _ => config.root.clone(),
            }),
        )
        .await?;

        let after_update_status =
            fpm::commands::status::get_file_status(config, &file, &snapshots, &workspaces).await?;

        if !before_update_status.eq(&after_update_status) {
            return Ok(EditResponse {
                path: path.to_string(),
                url: Some(format!(
                    "-/view-src/{}",
                    fpm::utils::path_with_root(file_name.as_str(), &root).trim_start_matches('/')
                )),
                reload: false,
            });
        }
    }

    Ok(EditResponse {
        path: path.to_string(),
        url,
        reload: false,
    })
}

async fn handle_cr_edit(
    config: &mut fpm::Config,
    request: &EditRequest,
    cr_number: usize,
    cr_path: &str,
) -> fpm::Result<EditResponse> {
    let mut tracks = fpm::tracker::get_cr_track(config, cr_number).await?;

    if request.is_delete() {
        let snapshot = fpm::snapshot::get_latest_snapshots(&config.root).await?;
        if let Some(timestamp) = snapshot.get(cr_path) {
            let mut cr_delete = fpm::cr::get_cr_delete(config, cr_number).await?;
            if !cr_delete.contains_key(cr_path) {
                let new_cr_delete = fpm::cr::CRDelete::new(cr_path, *timestamp);
                cr_delete.insert(cr_path.to_string(), new_cr_delete);
                fpm::cr::create_cr_delete(
                    config,
                    cr_delete.into_values().collect_vec().as_slice(),
                    cr_number,
                )
                .await?;
            }
        }
        if tracks.remove(cr_path).is_some() {
            fpm::tracker::create_cr_track(
                config,
                tracks.into_values().collect_vec().as_slice(),
                cr_number,
            )
            .await?;
        }

        return Ok(EditResponse {
            path: request.path.to_string(),
            url: None,
            reload: true,
        });
    }

    let edit_response = handle_add_modify(
        config,
        cr_path,
        Some(format!("-/{}", cr_number)),
        request.value.clone(),
    )
    .await?;
    if !tracks.contains_key(cr_path) {
        fpm::cr::add_cr_track(config, cr_path, &mut tracks).await?;
        fpm::tracker::create_cr_track(
            config,
            tracks.into_values().collect_vec().as_slice(),
            cr_number,
        )
        .await?;
    }

    Ok(edit_response)
}

pub async fn sync() -> actix_web::Result<actix_web::HttpResponse> {
    let config = match fpm::Config::read2(None, false).await {
        Ok(config) => config,
        Err(err) => {
            return fpm::apis::error(
                err.to_string(),
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    };
    match fpm::commands::sync::sync(&config, None).await {
        Ok(_) => {
            #[derive(serde::Serialize)]
            struct SyncResponse {
                reload: bool,
            }
            fpm::apis::success(SyncResponse { reload: true })
        }
        Err(err) => fpm::apis::error(
            err.to_string(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        ),
    }
}

#[derive(serde::Deserialize, serde::Serialize, std::fmt::Debug)]
pub struct RevertRequest {
    pub path: String,
}

pub async fn revert(
    req: actix_web::web::Json<RevertRequest>,
) -> actix_web::Result<actix_web::HttpResponse> {
    let config = match fpm::Config::read2(None, false).await {
        Ok(config) => config,
        Err(err) => {
            return fpm::apis::error(
                err.to_string(),
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            )
        }
    };

    match fpm::commands::revert::revert(&config, req.0.path.as_str()).await {
        Ok(_) => {
            #[derive(serde::Serialize)]
            struct RevertResponse {
                reload: bool,
            }
            fpm::apis::success(RevertResponse { reload: true })
        }
        Err(err) => fpm::apis::error(
            err.to_string(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        ),
    }
}
