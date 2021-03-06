use itertools::Itertools;

#[derive(serde::Deserialize, serde::Serialize, std::fmt::Debug, Clone)]
#[serde(tag = "action")]
pub enum SyncRequestFile {
    Add {
        path: String,
        content: Vec<u8>,
    },
    Update {
        path: String,
        content: Vec<u8>,
        version: i32,
    },
    Delete {
        path: String,
        version: i32,
    },
}

#[derive(serde::Deserialize, serde::Serialize, std::fmt::Debug)]
pub struct SyncRequest {
    pub package_name: String,
    pub files: Vec<SyncRequestFile>,
    pub history: String,
}

#[derive(serde::Serialize, serde::Deserialize, std::fmt::Debug)]
pub struct SyncResponse {
    pub files: Vec<SyncResponseFile>,
    pub dot_history: Vec<File>,
    pub latest_ftd: String,
}

#[derive(serde::Serialize, serde::Deserialize, std::fmt::Debug, PartialEq)]
pub enum SyncStatus {
    Conflict,
    NoConflict,
    ClientEditedServerDeleted,
    ClientDeletedServerEdited,
}

#[derive(serde::Serialize, serde::Deserialize, std::fmt::Debug)]
#[serde(tag = "action")]
pub enum SyncResponseFile {
    Add {
        path: String,
        status: SyncStatus,
        content: Vec<u8>,
    },
    Update {
        path: String,
        status: SyncStatus,
        content: Vec<u8>,
    },
    Delete {
        path: String,
        status: SyncStatus,
        content: Vec<u8>,
    },
}

impl SyncResponseFile {
    pub(crate) fn is_conflicted(&self) -> bool {
        let status = match self {
            SyncResponseFile::Add { status, .. }
            | SyncResponseFile::Update { status, .. }
            | SyncResponseFile::Delete { status, .. } => status,
        };
        if SyncStatus::NoConflict.eq(status) {
            return false;
        }
        true
    }

    pub(crate) fn is_deleted(&self) -> bool {
        matches!(self, SyncResponseFile::Delete { .. })
    }

    pub(crate) fn path(&self) -> String {
        match self {
            SyncResponseFile::Add { path, .. }
            | SyncResponseFile::Update { path, .. }
            | SyncResponseFile::Delete { path, .. } => path.to_string(),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, std::fmt::Debug)]
pub struct File {
    pub path: String,
    pub content: Vec<u8>,
}

pub async fn sync2(
    req: actix_web::web::Json<SyncRequest>,
) -> actix_web::Result<actix_web::HttpResponse> {
    dbg!("remote server call", &req.0.package_name);

    match sync_worker(req.0).await {
        Ok(data) => fpm::apis::success(data),
        Err(err) => fpm::apis::error(
            err.to_string(),
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
        ),
    }
}

pub(crate) async fn sync_worker(request: SyncRequest) -> fpm::Result<SyncResponse> {
    // TODO: Need to call at once only
    let config = fpm::Config::read2(None, false).await?;
    let mut server_history = config.get_history().await?;
    let server_latest =
        fpm::history::FileHistory::get_latest_file_edits(server_history.as_slice())?;
    let mut file_list: std::collections::BTreeMap<String, fpm::history::FileEditTemp> =
        Default::default();
    let mut synced_files = std::collections::HashMap::new();
    for file in request.files {
        match &file {
            SyncRequestFile::Add { path, content } => {
                // TODO: We need to check if, file is already available on server
                fpm::utils::update(&config.root.join(path), content).await?;
                // TODO: get all data like message, author, src-cr from request
                file_list.insert(
                    path.to_string(),
                    fpm::history::FileEditTemp {
                        message: None,
                        author: None,
                        src_cr: None,
                        operation: fpm::history::FileOperation::Added,
                    },
                );
            }
            SyncRequestFile::Update {
                path,
                content,
                version,
            } => {
                if let Some(file_edit) = server_latest.get(path) {
                    if file_edit.version.eq(version) {
                        fpm::utils::update(&config.root.join(path), content).await?;
                        // TODO: get all data like message, author, src-cr from request
                        file_list.insert(
                            path.to_string(),
                            fpm::history::FileEditTemp {
                                message: None,
                                author: None,
                                src_cr: None,
                                operation: fpm::history::FileOperation::Updated,
                            },
                        );
                    } else {
                        // else: Both has modified the same file
                        // TODO: Need to handle static files like images, don't require merging
                        let ancestor_path = config.history_path(path, *version);
                        let ancestor_content = tokio::fs::read_to_string(ancestor_path).await?;
                        let theirs_path = config.history_path(path, file_edit.version);
                        let theirs_content = tokio::fs::read_to_string(theirs_path).await?;
                        let ours_content = String::from_utf8(content.clone())
                            .map_err(|e| fpm::Error::APIResponseError(e.to_string()))?;
                        match diffy::MergeOptions::new()
                            .set_conflict_style(diffy::ConflictStyle::Merge)
                            .merge(&ancestor_content, &ours_content, &theirs_content)
                        {
                            Ok(data) => {
                                fpm::utils::update(&config.root.join(path), data.as_bytes())
                                    .await?;
                                file_list.insert(
                                    path.to_string(),
                                    fpm::history::FileEditTemp {
                                        message: None,
                                        author: None,
                                        src_cr: None,
                                        operation: fpm::history::FileOperation::Updated,
                                    },
                                );
                                synced_files.insert(
                                    path.to_string(),
                                    SyncResponseFile::Update {
                                        path: path.to_string(),
                                        status: SyncStatus::NoConflict,
                                        content: data.as_bytes().to_vec(),
                                    },
                                );
                            }
                            Err(data) => {
                                // Return conflicted content
                                synced_files.insert(
                                    path.to_string(),
                                    SyncResponseFile::Update {
                                        path: path.to_string(),
                                        status: SyncStatus::Conflict,
                                        content: data.as_bytes().to_vec(),
                                    },
                                );
                            }
                        }
                    }
                } else {
                    // else: Server don't have that file
                    // If client says edited and server says deleted
                    // That means at server side file is not present in latest
                    synced_files.insert(
                        path.to_string(),
                        SyncResponseFile::Update {
                            path: path.to_string(),
                            status: SyncStatus::ClientEditedServerDeleted,
                            content: content.clone(),
                        },
                    );
                    continue;
                };
            }
            SyncRequestFile::Delete { path, version } => {
                let file_edit = if let Some(file_edit) = server_latest.get(path) {
                    file_edit
                } else {
                    // ALready deleted in server, do nothing
                    continue;
                };
                let server_content =
                    tokio::fs::read(config.history_path(path, file_edit.version)).await?;

                // if: Client Says Deleted and server says modified
                // that means Remote timestamp is greater than client timestamp
                if file_edit.version.gt(version) {
                    synced_files.insert(
                        path.to_string(),
                        SyncResponseFile::Update {
                            path: path.to_string(),
                            status: SyncStatus::ClientDeletedServerEdited,
                            content: server_content,
                        },
                    );
                } else {
                    if config.root.join(path).exists() {
                        tokio::fs::remove_file(config.root.join(path)).await?;
                    }
                    file_list.insert(
                        path.to_string(),
                        fpm::history::FileEditTemp {
                            message: None,
                            author: None,
                            src_cr: None,
                            operation: fpm::history::FileOperation::Deleted,
                        },
                    );
                }
            }
        }
    }

    fpm::history::insert_into_history(&config.root, &file_list, &mut server_history).await?;

    let server_latest =
        fpm::history::FileHistory::get_latest_file_edits(server_history.as_slice())?;
    let client_history = fpm::history::FileHistory::from_ftd(request.history.as_str())?;
    let client_latest =
        fpm::history::FileHistory::get_latest_file_edits(client_history.as_slice())?;
    client_current_files(&config, &server_latest, &client_latest, &mut synced_files).await?;
    let history_files = client_history_files(&config, &server_latest, &client_latest).await?;
    let r = SyncResponse {
        files: synced_files.into_values().collect_vec(),
        dot_history: history_files,
        latest_ftd: tokio::fs::read_to_string(config.history_file()).await?,
    };
    Ok(r)
}

async fn client_history_files(
    config: &fpm::Config,
    server_latest: &std::collections::BTreeMap<String, fpm::history::FileEdit>,
    client_latest: &std::collections::BTreeMap<String, fpm::history::FileEdit>,
) -> fpm::Result<Vec<File>> {
    let diff = snapshot_diff(server_latest, client_latest);
    let history = ignore::WalkBuilder::new(config.server_history_dir())
        .build()
        .into_iter()
        .flatten()
        .map(|x| {
            x.into_path()
                .to_str()
                .unwrap()
                .trim_start_matches(config.server_history_dir().as_str())
                .trim_matches('/')
                .to_string()
        })
        .collect::<Vec<String>>();

    let mut dot_history = vec![];
    for ref path in diff {
        let client_file_edit = client_latest.get(path);
        let history_paths = get_all_versions(path, history.as_slice())?
            .into_iter()
            .filter(|x| client_file_edit.map(|c| x.0.gt(&c.version)).unwrap_or(true))
            .collect_vec();
        for (_, path) in history_paths {
            let content = tokio::fs::read(config.server_history_dir().join(&path)).await?;
            dot_history.push(File { path, content });
        }
    }
    Ok(dot_history)
}

fn get_all_versions(path: &str, history: &[String]) -> fpm::Result<Vec<(i32, String)>> {
    let (path_prefix, ext) = if let Some((path_prefix, ext)) = path.rsplit_once('.') {
        (format!("{}.", path_prefix), Some(ext))
    } else {
        (format!("{}.", path), None)
    };
    let mut versions = vec![];
    for path in history.iter().filter_map(|p| p.strip_prefix(&path_prefix)) {
        let (version, extension) = if let Some((version, extension)) = path.rsplit_once('.') {
            (version, Some(extension))
        } else {
            (path, None)
        };
        let version = version.parse::<i32>().unwrap();
        if ext.eq(&extension) {
            versions.push((version, format!("{}{}", path_prefix, path)));
        }
    }
    Ok(versions)
}

async fn client_current_files(
    config: &fpm::Config,
    server_latest: &std::collections::BTreeMap<String, fpm::history::FileEdit>,
    client_latest: &std::collections::BTreeMap<String, fpm::history::FileEdit>,
    synced_files: &mut std::collections::HashMap<String, SyncResponseFile>,
) -> fpm::Result<()> {
    let diff = snapshot_diff(server_latest, client_latest);
    for ref path in diff {
        if synced_files.contains_key(path) {
            continue;
        }
        let content = tokio::fs::read(config.root.join(path)).await?;
        synced_files.insert(
            path.clone(),
            SyncResponseFile::Add {
                path: path.clone(),
                status: SyncStatus::NoConflict,
                content,
            },
        );
    }

    // Deleted files
    let diff = client_latest
        .iter()
        .filter(|(path, _)| !server_latest.contains_key(path.as_str()));

    // TODO: If already in synced files need to handle that case
    for (path, _) in diff {
        if !synced_files.contains_key(path) {
            synced_files.insert(
                path.clone(),
                SyncResponseFile::Delete {
                    path: path.clone(),
                    status: SyncStatus::NoConflict,
                    content: vec![],
                },
            );
        }
    }
    Ok(())
}

fn snapshot_diff(
    server_latest: &std::collections::BTreeMap<String, fpm::history::FileEdit>,
    client_latest: &std::collections::BTreeMap<String, fpm::history::FileEdit>,
) -> Vec<String> {
    let mut diff = vec![];
    for (snapshot_path, file_edit) in server_latest {
        match client_latest.get(snapshot_path) {
            Some(client_file_edit) if client_file_edit.version.lt(&file_edit.version) => {
                diff.push(snapshot_path.to_string());
            }
            None => {
                diff.push(snapshot_path.to_string());
            }
            _ => {}
        };
    }
    diff
}
