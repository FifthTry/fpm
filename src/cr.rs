#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct CRAbout {
    pub title: String, // relative file name with respect to package root
    pub description: Option<String>,
    #[serde(rename = "cr-number")]
    pub cr_number: usize,
}

pub(crate) async fn get_cr_about(
    config: &fpm::Config,
    cr_number: usize,
) -> fpm::Result<fpm::cr::CRAbout> {
    let cr_about_path = config.cr_path(cr_number).join("-/about.ftd");
    if !cr_about_path.exists() {
        // TODO: should we error out here?
        return Ok(Default::default());
    }

    let doc = std::fs::read_to_string(&cr_about_path)?;
    resolve_cr_about(&doc).await
}

pub(crate) async fn resolve_cr_about(content: &str) -> fpm::Result<fpm::cr::CRAbout> {
    if content.trim().is_empty() {
        return Err(fpm::Error::UsageError {
            message: "Content is empty in cr about".to_string(),
        });
    }
    let lib = fpm::FPMLibrary::default();
    let b = match fpm::doc::parse_ftd(".about.ftd", content, &lib) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("failed to parse .latest.ftd: {:?}", &e);
            todo!();
        }
    };
    Ok(b.get("fpm#cr-about")?)
}

pub(crate) async fn create_cr_about(
    config: &fpm::Config,
    cr_about: &fpm::cr::CRAbout,
) -> fpm::Result<()> {
    let about_content = generate_cr_about_content(cr_about);
    fpm::utils::update(
        &config.cr_path(cr_about.cr_number),
        "-/about.ftd",
        about_content.as_bytes(),
    )
    .await?;
    Ok(())
}

pub(crate) fn generate_cr_about_content(cr_about: &fpm::cr::CRAbout) -> String {
    let mut about_content = format!(
        "-- import: fpm\n\n\n-- fpm.cr-about: {}\ncr-number: {}",
        cr_about.title, cr_about.cr_number
    );
    if let Some(ref description) = cr_about.description {
        about_content = format!("{}\n\n{}", about_content, description);
    }
    about_content
}

pub(crate) fn get_cr_and_path_from_id(id: &str, root: &Option<String>) -> Option<(usize, String)> {
    if let Some(path) = id.strip_prefix("-/") {
        let (cr_number, path) = if let Some((cr_number, path)) = path.split_once('/') {
            (cr_number, Some(path))
        } else {
            (path, None)
        };
        if let Ok(cr_number) = cr_number.parse::<usize>() {
            let path = match path {
                Some(path) if !path.is_empty() => path.to_string(),
                _ => "/".to_string(),
            };
            return Some((cr_number, path));
        }
    }

    if let Some(root) = root {
        if let Some((cr_number, r)) = get_cr_and_path_from_id(root, &None) {
            let mut r = format!("{}/{}", r.trim_end_matches('/'), id.trim_start_matches('/'));
            if !r.eq("/") {
                r = r.trim_start_matches('/').to_string();
            }
            return Some((cr_number, r));
        }
    }
    None
}

pub(crate) fn get_cr_special_ids() -> Vec<String> {
    vec!["-/about".to_string(), "-/about.ftd".to_string()]
}

pub(crate) fn is_about(path: &str) -> bool {
    ["-/about", "-/about.ftd"].contains(&path.trim_end_matches('/'))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub enum Operation {
    Add,
    Delete,
    Modify,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Track {
    #[serde(rename = "file-name")]
    pub file_name: String,
    #[serde(rename = "self-timestamp")]
    pub self_timestamp: Option<u128>,
    pub base: Option<u128>,
    pub operation: Operation,
}

pub(crate) async fn add_cr_track(
    config: &fpm::Config,
    path: &str,
    tracks: &mut std::collections::BTreeMap<String, fpm::Track>,
) -> fpm::Result<()> {
    let snapshot = fpm::snapshot::get_latest_snapshots(&config.root).await?;
    let track = fpm::Track::new(path).set_other_timestamp(snapshot.get(path).map(|v| v.to_owned()));
    tracks.insert(path.to_string(), track);
    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
pub struct CRDelete {
    pub filename: String,
    pub timestamp: u128,
}

impl CRDelete {
    pub fn new(filename: &str, timestamp: u128) -> fpm::cr::CRDelete {
        fpm::cr::CRDelete {
            filename: filename.to_string(),
            timestamp,
        }
    }
}

pub(crate) async fn get_cr_delete(
    config: &fpm::Config,
    cr_number: usize,
) -> fpm::Result<std::collections::BTreeMap<String, fpm::cr::CRDelete>> {
    let cr_delete_path = config.cr_path(cr_number).join(".delete.ftd");
    if !cr_delete_path.exists() {
        // TODO: should we error out here?
        return Ok(Default::default());
    }

    let doc = std::fs::read_to_string(&cr_delete_path)?;
    resolve_cr_delete(&doc).await
}

pub(crate) async fn resolve_cr_delete(
    content: &str,
) -> fpm::Result<std::collections::BTreeMap<String, fpm::cr::CRDelete>> {
    if content.trim().is_empty() {
        return Err(fpm::Error::UsageError {
            message: "Content is empty in cr about".to_string(),
        });
    }
    let lib = fpm::FPMLibrary::default();
    let b = match fpm::doc::parse_ftd(".delete.ftd", content, &lib) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("failed to parse .latest.ftd: {:?}", &e);
            todo!();
        }
    };
    let mut deletes = std::collections::BTreeMap::new();
    let delete_list: Vec<fpm::cr::CRDelete> = b.get("fpm#cr-delete")?;
    for delete in delete_list {
        deletes.insert(delete.filename.to_string(), delete);
    }
    Ok(deletes)
}

pub(crate) async fn create_cr_delete(
    config: &fpm::Config,
    cr_delete: &[CRDelete],
    cr_number: usize,
) -> fpm::Result<()> {
    let delete_content = generate_cr_delete_content(cr_delete);
    fpm::utils::update(
        &config.cr_path(cr_number),
        ".delete.ftd",
        delete_content.as_bytes(),
    )
    .await?;
    Ok(())
}

pub(crate) fn generate_cr_delete_content(cr_delete: &[CRDelete]) -> String {
    let mut delete_content = "-- import: fpm".to_string();
    for delete in cr_delete {
        let delete_data = format!(
            "-- fpm.cr-delete: {}\ntimestamp: {}\n\n",
            delete.filename, delete.timestamp
        );
        delete_content = format!("{}\n\n\n{}", delete_content, delete_data);
    }
    delete_content
}

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct PackageTocItem {
    pub url: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "status-with-last-sync")]
    pub status_with_last_sync: Option<String>,
    pub title: Option<String>,
    pub path: Option<String>,
    pub children: Vec<PackageTocItem>,
}

impl PackageTocItem {
    pub(crate) fn new(url: Option<String>, title: Option<String>) -> PackageTocItem {
        PackageTocItem {
            url,
            status: None,
            status_with_last_sync: None,
            title,
            path: None,
            children: vec![],
        }
    }

    pub(crate) fn add_path(mut self, path: &str) -> Self {
        self.path = Some(path.to_string());
        self
    }
}
