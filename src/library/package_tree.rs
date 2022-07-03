use itertools::Itertools;

pub enum PackageType {
    CR(usize),
    Current,
}

pub async fn processor<'a>(
    section: &ftd::p1::Section,
    doc: &ftd::p2::TDoc<'a>,
    config: &fpm::Config,
) -> ftd::p1::Result<ftd::Value> {
    processor_(section, doc, config)
        .await
        .map_err(|e| ftd::p1::Error::ParseError {
            message: e.to_string(),
            doc_id: doc.name.to_string(),
            line_number: section.line_number,
        })
}

pub async fn processor_<'a>(
    section: &ftd::p1::Section,
    doc: &ftd::p2::TDoc<'a>,
    config: &fpm::Config,
) -> fpm::Result<ftd::Value> {
    if let Some(ref id) = config.current_document {
        if let Some((cr_number, _)) = fpm::cr::get_cr_and_path_from_id(id, &None) {
            return cr_processor(section, doc, config, cr_number).await;
        }
    }
    root_processor(section, doc, config).await
}

pub async fn cr_processor<'a>(
    section: &ftd::p1::Section,
    doc: &ftd::p2::TDoc<'a>,
    config: &fpm::Config,
    cr_number: usize,
) -> fpm::Result<ftd::Value> {
    let root = config.get_root_for_package(&config.package);
    let snapshots = fpm::snapshot::get_latest_snapshots(&config.root).await?;
    let workspaces = fpm::snapshot::get_cr_workspace(config, cr_number).await?;

    let mut files = config
        .get_all_file_paths(&config.package, true)?
        .into_iter()
        .filter(|v| v.is_file())
        .map(|v| {
            v.strip_prefix(&root)
                .unwrap_or_else(|_| v.as_path())
                .to_string()
                .replace(std::path::MAIN_SEPARATOR.to_string().as_str(), "/")
        })
        .collect_vec();

    let cr_files = files
        .iter()
        .filter_map(|v| {
            v.strip_prefix(format!("-/{}/", cr_number).as_str())
                .map(|v| v.to_string())
        })
        .filter(|v| !fpm::cr::get_cr_special_ids().contains(&v.trim_matches('/').to_string()))
        .map(|v| (v, Some(format!("-/{}", cr_number))))
        .collect_vec();

    files = files
        .iter()
        .filter(|v| !v.starts_with("-/"))
        .map(|v| v.to_string())
        .collect_vec();

    let mut file_map = files
        .into_iter()
        .map(|v| (v, Some(format!("-/{}", cr_number))))
        .collect::<std::collections::BTreeMap<String, Option<String>>>();
    file_map.extend(cr_files);

    let mut file_map = file_map.into_iter().collect_vec();
    file_map.sort();

    let tree = construct_tree(
        config,
        file_map.as_slice(),
        &snapshots,
        &workspaces,
        PackageType::CR(cr_number),
    )
    .await?;
    Ok(doc.from_json(&tree, section)?)
}

pub async fn root_processor<'a>(
    section: &ftd::p1::Section,
    doc: &ftd::p2::TDoc<'a>,
    config: &fpm::Config,
) -> fpm::Result<ftd::Value> {
    let root = config.get_root_for_package(&config.package);
    let snapshots = fpm::snapshot::get_latest_snapshots(&config.root).await?;
    let workspaces = fpm::snapshot::get_workspace(config).await?;
    let all_files = config
        .get_files(&config.package)
        .await?
        .into_iter()
        .map(|v| v.get_id())
        .collect_vec();
    let deleted_files = snapshots
        .keys()
        .filter(|v| !all_files.contains(v))
        .map(|v| v.to_string());

    let mut files = config
        .get_all_file_paths(&config.package, true)?
        .into_iter()
        .filter(|v| v.is_file())
        .map(|v| {
            v.strip_prefix(&root)
                .unwrap_or_else(|_| v.as_path())
                .to_string()
                .replace(std::path::MAIN_SEPARATOR.to_string().as_str(), "/")
        })
        .collect_vec();
    files.extend(deleted_files);

    files = files
        .into_iter()
        .filter(|v| !v.starts_with("-/"))
        .collect_vec();

    files.sort();

    let tree = construct_tree(
        config,
        files
            .into_iter()
            .map(|v| (v, None))
            .collect_vec()
            .as_slice(),
        &snapshots,
        &workspaces,
        PackageType::Current,
    )
    .await?;
    Ok(doc.from_json(&tree, section)?)
}

async fn construct_tree(
    config: &fpm::Config,
    files: &[(String, Option<String>)],
    snapshots: &std::collections::BTreeMap<String, u128>,
    workspaces: &std::collections::BTreeMap<String, fpm::snapshot::Workspace>,
    package_type: PackageType,
) -> fpm::Result<Vec<fpm::cr::PackageTocItem>> {
    let mut tree = vec![];
    for (file, root) in files {
        let root = if let Some(root) = root {
            format!("{}/", root.trim_matches('/'))
        } else {
            "".to_string()
        };

        insert(
            config,
            &mut tree,
            file,
            format!("-/view-src/{}{}", root, file.trim_start_matches('/')).as_str(),
            file,
            snapshots,
            workspaces,
            &package_type,
        )
        .await?;
    }
    Ok(tree)
}

#[async_recursion::async_recursion(?Send)]
async fn insert(
    config: &fpm::Config,
    tree: &mut Vec<fpm::cr::PackageTocItem>,
    path: &str,
    url: &str,
    full_path: &str,
    snapshots: &std::collections::BTreeMap<String, u128>,
    workspaces: &std::collections::BTreeMap<String, fpm::snapshot::Workspace>,
    package_type: &PackageType,
) -> fpm::Result<()> {
    let (path, rest) = if let Some((path, rest)) = path.split_once('/') {
        (path, Some(rest))
    } else {
        (path, None)
    };

    let node = if let Some(node) = tree
        .iter_mut()
        .find(|node| node.title.as_ref().map(|v| v.eq(path)).unwrap_or(false))
    {
        node
    } else {
        let full_path = rest
            .map(|v| full_path.trim_end_matches(v))
            .unwrap_or(full_path);
        tree.push(fpm::cr::PackageTocItem::new(None, Some(path.to_string())).add_path(full_path));
        tree.last_mut().unwrap()
    };

    if let Some(rest) = rest {
        insert(
            config,
            &mut node.children,
            rest,
            url,
            full_path,
            snapshots,
            workspaces,
            package_type,
        )
        .await?;
    } else {
        set_status(
            config,
            node,
            url,
            full_path,
            snapshots,
            workspaces,
            package_type,
        )
        .await?;
    }

    Ok(())
}

async fn set_status(
    config: &fpm::Config,
    node: &mut fpm::cr::PackageTocItem,
    url: &str,
    full_path: &str,
    snapshots: &std::collections::BTreeMap<String, u128>,
    workspaces: &std::collections::BTreeMap<String, fpm::snapshot::Workspace>,
    package_type: &PackageType,
) -> fpm::Result<()> {
    if let PackageType::CR(cr_number) = package_type {
        let cr_tracks = fpm::tracker::get_cr_track(config, *cr_number).await?;
        let cr_delete = fpm::cr::get_cr_delete(config, *cr_number).await?;

        if let Some(track) = cr_tracks.get(full_path) {
            if track.other_timestamp.is_some() {
                node.status = Some(format!("{:?}", fpm::commands::status::FileStatus::Modified));
                node.url = Some(url.to_string());
            } else {
                node.status = Some(format!("{:?}", fpm::commands::status::FileStatus::Added));
                node.url = Some(url.to_string());
            }
        } else if cr_delete.contains_key(full_path) {
            node.status = Some(format!("{:?}", fpm::commands::status::FileStatus::Deleted));
        } else if let Ok(file) = fpm::get_file(
            config.package.name.to_string(),
            &config.root.join(full_path),
            &config.cr_path(*cr_number),
        )
        .await
        {
            let status =
                fpm::commands::status::get_file_status(config, &file, snapshots, workspaces)
                    .await?;
            node.url = Some(url.to_string());
            node.status = Some(format!("{:?}", status))
        } else {
            node.url = Some(url.to_string());
            node.status = Some(format!(
                "{:?}",
                fpm::commands::status::FileStatus::Untracked
            ));
        }
        return Ok(());
    }

    if let Ok(file) = fpm::get_file(
        config.package.name.to_string(),
        &config.root.join(full_path),
        &config.root,
    )
    .await
    {
        let status =
            fpm::commands::status::get_file_status(config, &file, snapshots, workspaces).await?;
        node.url = Some(url.to_string());
        node.status = Some(format!("{:?}", status))
    } else {
        node.status = Some(format!("{:?}", fpm::commands::status::FileStatus::Deleted))
    }
    Ok(())
}
