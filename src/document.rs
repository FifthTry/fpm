#[derive(Debug, Clone)]
pub enum File {
    Ftd(Document),
    Static(Static),
    Markdown(Document),
}

impl File {
    pub fn get_id(&self) -> String {
        match self {
            Self::Ftd(a) => a.id.clone(),
            Self::Static(a) => a.id.clone(),
            Self::Markdown(a) => a.id.clone(),
        }
    }
    pub fn get_base_path(&self) -> String {
        match self {
            Self::Ftd(a) => a.parent_path.to_string(),
            Self::Static(a) => a.base_path.to_string(),
            Self::Markdown(a) => a.parent_path.to_string(),
        }
    }
    pub fn get_full_path(&self) -> camino::Utf8PathBuf {
        let (id, base_path) = match self {
            Self::Ftd(a) => (a.id.to_string(), a.parent_path.to_string()),
            Self::Static(a) => (a.id.to_string(), a.base_path.to_string()),
            Self::Markdown(a) => (a.id.to_string(), a.parent_path.to_string()),
        };
        camino::Utf8PathBuf::from(base_path).join(id)
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub package_name: String,
    pub id: String,
    pub content: String,
    pub parent_path: String,
}

impl Document {
    pub fn id_to_path(&self) -> String {
        self.id
            .replace(".ftd", std::path::MAIN_SEPARATOR.to_string().as_str())
    }

    pub fn id_with_package(&self) -> String {
        format!(
            "{}/{}",
            self.package_name.as_str(),
            self.id.trim_end_matches(".ftd")
        )
    }
}

#[derive(Debug, Clone)]
pub struct Static {
    pub id: String,
    pub base_path: camino::Utf8PathBuf,
}

pub(crate) async fn get_documents(config: &fpm::Config) -> fpm::Result<Vec<fpm::File>> {
    let mut ignore_paths = ignore::WalkBuilder::new("./");
    ignore_paths.overrides(package_ignores(config)?);
    let all_files = ignore_paths
        .build()
        .into_iter()
        .flatten()
        .map(|x| camino::Utf8PathBuf::from_path_buf(x.into_path()).unwrap()) //todo: improve error message
        .collect::<Vec<camino::Utf8PathBuf>>();
    let mut documents =
        fpm::paths_to_files(config.package.name.as_str(), all_files, &config.root).await?;
    documents.sort_by_key(|v| v.get_id());

    Ok(documents)
}

pub(crate) async fn paths_to_files(
    package_name: &str,
    files: Vec<camino::Utf8PathBuf>,
    base_path: &camino::Utf8Path,
) -> fpm::Result<Vec<fpm::File>> {
    let pkg = package_name.to_string();
    Ok(futures::future::join_all(
        files
            .into_iter()
            .map(|x| {
                let base = base_path.to_path_buf();
                let p = pkg.clone();
                tokio::spawn(async move { fpm::get_file(p, &x, &base).await })
            })
            .collect::<Vec<tokio::task::JoinHandle<fpm::Result<fpm::File>>>>(),
    )
    .await
    .into_iter()
    .flatten()
    .flatten()
    .collect::<Vec<fpm::File>>())
}

pub fn package_ignores(config: &fpm::Config) -> Result<ignore::overrides::Override, ignore::Error> {
    let mut overrides = ignore::overrides::OverrideBuilder::new("./");
    overrides.add("!.history")?;
    overrides.add("!.packages")?;
    overrides.add("!.tracks")?;
    overrides.add("!FPM")?;
    overrides.add("!rust-toolchain")?;
    overrides.add("!.build")?;
    for ignored_path in &config.package.ignored_paths {
        overrides.add(format!("!{}", ignored_path).as_str())?;
    }
    overrides.build()
}

pub(crate) async fn get_file(
    package_name: String,
    doc_path: &camino::Utf8Path,
    base_path: &camino::Utf8Path,
) -> fpm::Result<File> {
    if doc_path.is_dir() {
        return Err(fpm::Error::UsageError {
            message: format!("{} should be a file", doc_path.as_str()),
        });
    }

    let id = match std::fs::canonicalize(doc_path)?
        .to_str()
        .unwrap()
        .rsplit_once(
            if base_path.as_str().ends_with(std::path::MAIN_SEPARATOR) {
                base_path.as_str().to_string()
            } else {
                format!("{}{}", base_path, std::path::MAIN_SEPARATOR)
            }
            .as_str(),
        ) {
        Some((_, id)) => id.to_string(),
        None => {
            return Err(fpm::Error::UsageError {
                message: format!("{:?} should be a file", doc_path),
            });
        }
    };

    Ok(match id.rsplit_once(".") {
        Some((_, "ftd")) => File::Ftd(Document {
            package_name: package_name.to_string(),
            id: id.to_string(),
            content: tokio::fs::read_to_string(&doc_path).await?,
            parent_path: base_path.to_string(),
        }),
        Some((_, "md")) => File::Markdown(Document {
            package_name: package_name.to_string(),
            id: id.to_string(),
            content: tokio::fs::read_to_string(&doc_path).await?,
            parent_path: base_path.to_string(),
        }),
        _ => File::Static(Static {
            id: id.to_string(),
            base_path: base_path.to_path_buf(),
        }),
    })
}
