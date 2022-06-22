pub async fn sync(config: &fpm::Config, files: Option<Vec<String>>) -> fpm::Result<()> {
    // Read All the Document
    // Get all the updated, added and deleted files
    // Get Updated Files -> If content differs from latest snapshot
    // Get Added Files -> If files does not present in latest snapshot
    // Get Deleted Files -> If file present in latest.ftd and not present in directory
    // Send to fpm server

    let documents = if let Some(ref files) = files {
        let files = files
            .to_vec()
            .into_iter()
            .map(|x| config.root.join(x))
            .collect::<Vec<camino::Utf8PathBuf>>();
        fpm::paths_to_files(config.package.name.as_str(), files, config.root.as_path()).await?
    } else {
        config.get_files(&config.package).await?
    };

    tokio::fs::create_dir_all(config.history_dir()).await?;

    let snapshots = fpm::snapshot::get_latest_snapshots(&config.root).await?;

    let latest_ftd = tokio::fs::read_to_string(config.history_dir().join(".latest.ftd"))
        .await
        .unwrap_or("latest.ftd".to_string());

    let changed_files = get_changed_files(&documents, &snapshots).await?;

    let request = fpm::apis::sync::SyncRequest {
        files: changed_files,
        package_name: config.package.name.to_string(),
        latest_ftd,
    };

    let response = send_to_fpm_serve(request).await?;

    // Tumhe nahi chalana hai mujhe to, koi aur chalaye to chalaye
    if false {
        let timestamp = fpm::get_timestamp_nanosecond();
        let mut modified_files = vec![];
        let mut new_snapshots = vec![];
        for doc in documents {
            let (snapshot, is_modified) = write(&doc, timestamp, &snapshots).await?;
            if is_modified {
                modified_files.push(snapshot.filename.to_string());
            }
            new_snapshots.push(snapshot);
        }

        if let Some(file) = files {
            let snapshot_id = new_snapshots
                .iter()
                .map(|v| v.filename.to_string())
                .collect::<Vec<String>>();
            for (k, timestamp) in snapshots.iter() {
                if !snapshot_id.contains(k) && file.contains(k) {
                    continue;
                }
                if !snapshot_id.contains(k) {
                    new_snapshots.push(fpm::Snapshot {
                        filename: k.clone(),
                        timestamp: *timestamp,
                    })
                }
            }
        }

        for key in snapshots.keys() {
            if new_snapshots.iter().filter(|v| v.filename.eq(key)).count() == 0 {
                modified_files.push(key.clone());
            }
        }

        if modified_files.is_empty() {
            println!("Everything is upto date.");
        } else {
            fpm::snapshot::create_latest_snapshots(config, &new_snapshots).await?;
            println!(
                "Repo for {} is github, directly syncing with .history.",
                config.package.name
            );
            for file in modified_files {
                println!("{}", file);
            }
        }
    }
    Ok(())
}

async fn get_changed_files(
    files: &[fpm::File],
    snapshots: &std::collections::BTreeMap<String, u128>,
) -> fpm::Result<Vec<fpm::apis::sync::SyncRequestFile>> {
    use sha2::Digest;
    // Get all the updated, added and deleted files
    // Get Updated Files -> If content differs from latest snapshot
    // Get Added Files -> If files does not present in latest snapshot
    // Get Deleted Files -> If file present in latest.ftd and not present in files directory

    let mut changed_files = Vec::new();
    for document in files.iter() {
        if let Some(timestamp) = snapshots.get(&document.get_id()) {
            let snapshot_file_path =
                fpm::utils::history_path(&document.get_id(), &document.get_base_path(), timestamp);
            let snapshot_file_content = tokio::fs::read(&snapshot_file_path).await?;
            // Update
            let current_file_content = document.get_content();
            if sha2::Sha256::digest(&snapshot_file_content)
                .eq(&sha2::Sha256::digest(&current_file_content))
            {
                continue;
            }

            changed_files.push(fpm::apis::sync::SyncRequestFile::Update {
                path: document.get_id(),
                content: current_file_content,
            });
        } else {
            // Added
            changed_files.push(fpm::apis::sync::SyncRequestFile::Add {
                path: document.get_id(),
                content: document.get_content(),
            });
        }
    }
    let files_path = files
        .iter()
        .map(|f| f.get_id())
        .collect::<std::collections::HashSet<String>>();

    let deleted_files = snapshots
        .keys()
        .filter(|x| !files_path.contains(*x))
        .map(|f| fpm::apis::sync::SyncRequestFile::Delete {
            path: f.to_string(),
        });

    changed_files.extend(deleted_files);

    Ok(changed_files)
}

async fn write(
    doc: &fpm::File,
    timestamp: u128,
    snapshots: &std::collections::BTreeMap<String, u128>,
) -> fpm::Result<(fpm::Snapshot, bool)> {
    use sha2::Digest;
    if let Some((dir, _)) = doc.get_id().rsplit_once('/') {
        tokio::fs::create_dir_all(
            camino::Utf8PathBuf::from(doc.get_base_path())
                .join(".history")
                .join(dir),
        )
        .await?;
    }

    if let Some(timestamp) = snapshots.get(&doc.get_id()) {
        let path = fpm::utils::history_path(&doc.get_id(), &doc.get_base_path(), timestamp);
        if let Ok(current_doc) = tokio::fs::read(&doc.get_full_path()).await {
            let existing_doc = tokio::fs::read(&path).await?;

            if sha2::Sha256::digest(current_doc).eq(&sha2::Sha256::digest(existing_doc)) {
                return Ok((
                    fpm::Snapshot {
                        filename: doc.get_id(),
                        timestamp: *timestamp,
                    },
                    false,
                ));
            }
        }
    }

    let new_file_path = fpm::utils::history_path(&doc.get_id(), &doc.get_base_path(), &timestamp);

    tokio::fs::copy(doc.get_full_path(), new_file_path).await?;

    Ok((
        fpm::Snapshot {
            filename: doc.get_id(),
            timestamp,
        },
        true,
    ))
}
// fpm::apis::sync::SyncResponse
async fn send_to_fpm_serve(data: fpm::apis::sync::SyncRequest) -> fpm::Result<()> {
    // println!("Data send {:#?}", data);
    let data = serde_json::to_string(&data)?;
    dbg!(&data.as_bytes().len());
    let response = reqwest::Client::new()
        .post("http://127.0.0.1:8000/-/sync/")
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(data)
        .send()?;

    dbg!(response);

    Ok(())
}

async fn handle_sync_response(response: fpm::apis::sync::SyncResponse) {
    println!("{:?}", response);
}
