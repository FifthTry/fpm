use std::convert::TryInto;

/// `Config` struct keeps track of a few configuration parameters that is shared with the entire
/// program. It is constructed from the content of `FPM.ftd` file for the package.
///
/// `Config` is created using `Config::read()` method, and should be constructed only once in the
/// `main()` and passed everywhere.
#[derive(Debug, Clone)]
pub struct Config {
    pub package: fpm::Package,
    /// `root` is the package root folder, this is the folder where `FPM.ftd` file is stored.
    ///
    /// Technically the rest of the program can simply call `std::env::current_dir()` and that
    /// is guaranteed to be same as `Config.root`, but `Config.root` is camino path, instead of
    /// std::path::Path, so we can treat `root` as a handy helper.
    ///
    /// A utility that returns camino version of `current_dir()` may be used in future.
    pub root: camino::Utf8PathBuf,
    /// `original_directory` is the directory from which the `fpm` command was invoked
    ///
    /// During the execution of `fpm`, we change the directory to the package root so the program
    /// can be written with the assumption that they are running from package `root`.
    ///
    /// When printing filenames for users consumption we want to print the paths relative to the
    /// `original_directory`, so we keep track of the original directory.
    pub original_directory: camino::Utf8PathBuf,
    /// `fonts` keeps track of the fonts used by the package.
    ///
    /// Note that this too is kind of bad design, we will move fonts to `fpm::Package` struct soon.
    pub fonts: Vec<fpm::Font>,
    /// `ignored` keeps track of files that are to be ignored by `fpm build`, `fpm sync` etc.
    pub ignored: ignore::overrides::Override,
}

impl Config {
    /// `build_dir` is where the static built files are stored. `fpm build` command creates this
    /// folder and stores its output here.
    pub fn build_dir(&self) -> camino::Utf8PathBuf {
        self.root.join(".build")
    }

    /// history of a fpm package is stored in `.history` folder.
    ///
    /// Current design is wrong, we should move this helper to `fpm::Package` maybe.
    ///
    /// History of a package is considered part of the package, and when a package is downloaded we
    /// have to chose if we want to download its history as well. For now we do not. Eventually in
    /// we will be able to say download the history also for some package.
    ///
    /// ```ftd
    /// -- ftp.dependency: django
    ///  with-history: true
    /// ```
    ///     
    /// `.history` file is created or updated by `fpm sync` command only, no one else should edit
    /// anything in it.
    pub fn history_dir(&self) -> camino::Utf8PathBuf {
        self.root.join(".history")
    }

    /// every package's `.history` contains a file `.latest.ftd`. It looks a bit linke this:
    ///
    /// ```ftd
    /// -- import: fpm
    ///
    /// -- fpm.snapshot: FPM.ftd
    /// timestamp: 1638706756293421000
    ///
    /// -- fpm.snapshot: blog.ftd
    /// timestamp: 1638706756293421000
    /// ```
    ///
    /// One `fpm.snapshot` for every file that is currently part of the package.
    pub fn latest_ftd(&self) -> camino::Utf8PathBuf {
        self.root.join(".history/.latest.ftd")
    }

    /// track_dir returns the directory where track files are stored. Tracking information as well
    /// is considered part of a package, but it is not downloaded when a package is downloaded as
    /// a dependency of another package.
    pub fn track_dir(&self) -> camino::Utf8PathBuf {
        self.root.join(".tracks")
    }

    /// `is_translation_package()` is a helper to tell you if the current package is a translation
    /// of another package. We may delete this helper soon.
    pub fn is_translation_package(&self) -> bool {
        self.package.translation_of.is_some()
    }

    /// original_path() returns the path of the original package if the current package is a
    /// translation package. it returns the path in `.packages` folder where the
    pub fn original_path(&self) -> fpm::Result<camino::Utf8PathBuf> {
        let o = match self.package.translation_of.as_ref() {
            Some(ref o) => o,
            None => {
                return Err(fpm::Error::UsageError {
                    message: "This package is not a translation package".to_string(),
                });
            }
        };
        Ok(self.root.join(".packages").join(o.name.as_str()))
    }

    /*/// aliases() returns the list of the available aliases at the package level.
    pub fn aliases(&self) -> fpm::Result<std::collections::BTreeMap<&str, &fpm::Package>> {
        let mut resp = std::collections::BTreeMap::new();
        self.package
            .dependencies
            .iter()
            .filter(|d| d.alias.is_some())
            .for_each(|d| {
                resp.insert(d.alias.as_ref().unwrap().as_str(), &d.package);
            });
        Ok(resp)
    }*/

    /// `get_font_style()` returns the HTML style tag which includes all the fonts used by any
    /// ftd document. Currently this function does not check for fonts in package dependencies
    /// nor it tries to avoid fonts that are configured but not needed in current document.
    pub fn get_font_style(&self) -> String {
        // TODO: accept list of actual fonts used in the current document. each document accepts
        //       a different list of fonts and only fonts used by a given document should be
        //       included in the HTML produced by that font
        // TODO: fetch fonts from package dependencies as well (ideally this function should fail
        //       if one of the fonts used by any ftd document is not found
        let generated_style = self
            .fonts
            .iter()
            .fold("".to_string(), |c, f| format!("{}\n{}", c, f.to_html()));
        return match generated_style.is_empty() {
            false => format!("<style>{}</style>", generated_style),
            _ => "".to_string(),
        };
    }

    /// `read()` is the way to read a Config.
    pub async fn read() -> fpm::Result<Config> {
        let original_directory: camino::Utf8PathBuf =
            std::env::current_dir()?.canonicalize()?.try_into()?;
        let root = match find_package_root(&original_directory) {
            Some(b) => b,
            None => {
                return Err(fpm::Error::UsageError {
                    message: "FPM.ftd not found in any parent directory".to_string(),
                });
            }
        };
        let b = {
            let doc = tokio::fs::read_to_string(root.join("FPM.ftd"));
            let lib = fpm::FPMLibrary::default();
            match ftd::p2::Document::from("FPM", doc.await?.as_str(), &lib) {
                Ok(v) => v,
                Err(e) => {
                    return Err(fpm::Error::PackageError {
                        message: format!("failed to parse FPM.ftd 3: {:?}", &e),
                    });
                }
            }
        };

        let deps = {
            let temp_deps: Vec<fpm::dependency::DependencyTemp> = b.get("fpm#dependency")?;
            temp_deps
                .into_iter()
                .map(|v| v.into_dependency())
                .collect::<Vec<fpm::Result<fpm::Dependency>>>()
                .into_iter()
                .collect::<fpm::Result<Vec<fpm::Dependency>>>()?
        };

        let mut package = {
            let temp_package: PackageTemp = b.get("fpm#package")?;
            let mut package = temp_package.into_package();
            package.dependencies = deps;
            package
        };

        let fonts: Vec<fpm::Font> = b.get("fpm#font")?;

        fpm::utils::validate_zip_url(&package)?;

        let ignored = {
            let mut overrides = ignore::overrides::OverrideBuilder::new("./");
            for ig in b.get::<Vec<String>>("fpm#ignore")? {
                if let Err(e) = overrides.add(format!("!{}", ig.as_str()).as_str()) {
                    return Err(fpm::Error::PackageError {
                        message: format!("failed parse fpm.ignore: {} => {:?}", ig, e),
                    });
                }
            }

            match overrides.build() {
                Ok(v) => v,
                Err(e) => {
                    return Err(fpm::Error::PackageError {
                        message: format!("failed parse fpm.ignore: {:?}", e),
                    });
                }
            }
        };

        fpm::dependency::ensure(&root, &mut package)?;
        dbg!(&package);
        Ok(Config {
            package,
            root,
            original_directory,
            fonts,
            ignored,
        })
    }
}

/// `find_package_root()` starts with the given path, which is the current directory where the
/// application started in, and goes up till it finds a folder that contains `FPM.ftd` file.
pub(crate) fn find_package_root(dir: &camino::Utf8Path) -> Option<camino::Utf8PathBuf> {
    if dir.join("FPM.ftd").exists() {
        Some(dir.into())
    } else {
        if let Some(p) = dir.parent() {
            return find_package_root(p);
        };
        None
    }
}

/// PackageTemp is a struct that is used for mapping the `fpm.package` data in FPM.ftd file. It is
/// not used elsewhere in program, it is immediately converted to `fpm::Package` struct during
/// deserialization process
#[derive(serde::Deserialize, Debug, Clone)]
pub(crate) struct PackageTemp {
    pub name: String,
    #[serde(rename = "translation-of")]
    pub translation_of: Option<String>,
    #[serde(rename = "translation")]
    pub translations: Vec<String>,
    #[serde(rename = "language")]
    pub language: Option<String>,
    pub about: Option<String>,
    pub zip: Option<String>,
    #[serde(rename = "canonical-url")]
    pub canonical_url: Option<String>,
    #[serde(rename = "main")]
    pub main_aliases: Option<String>,
}

impl PackageTemp {
    pub fn into_package(self) -> fpm::Package {
        // TODO: change this method to: `validate(self) -> fpm::Result<fpm::Package>` and do all
        //       validations in it. Like a package must not have both translation-of and
        //       `translations` set.
        let translation_of = self.translation_of.as_ref().map(|v| fpm::Package::new(v));
        let translations = self
            .translations
            .clone()
            .into_iter()
            .map(|v| fpm::Package::new(&v))
            .collect::<Vec<fpm::Package>>();

        let interface_aliases = match self.main_aliases {
            Some(i) => i
                .split(",")
                .into_iter()
                .map(|x| x.trim().to_string())
                .collect::<Vec<String>>(),
            None => vec![],
        };

        fpm::Package {
            name: self.name,
            translation_of: Box::new(translation_of),
            translations,
            language: self.language,
            about: self.about,
            zip: self.zip,
            translation_status: None,
            canonical_url: self.canonical_url,
            dependencies: vec![],
            interface_aliases,
        }
    }
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct Package {
    pub name: String,
    pub translation_of: Box<Option<Package>>,
    pub translations: Vec<Package>,
    pub language: Option<String>,
    pub about: Option<String>,
    pub zip: Option<String>,
    pub translation_status: Option<fpm::translation::TranslationStatusCount>,
    pub canonical_url: Option<String>,
    /// `dependencies` keeps track of direct dependencies of a given package. This too should be
    /// moved to `fpm::Package` to support recursive dependencies etc.
    pub dependencies: Vec<fpm::Dependency>,
    pub interface_aliases: Vec<String>,
}

impl Package {
    pub fn new(name: &str) -> fpm::Package {
        fpm::Package {
            name: name.to_string(),
            translation_of: Box::new(None),
            translations: vec![],
            language: None,
            about: None,
            zip: None,
            translation_status: None,
            canonical_url: None,
            dependencies: vec![],
            interface_aliases: vec![],
        }
    }

    pub fn generate_canonical_url(&self, path: &str) -> String {
        match &self.canonical_url {
            Some(url) => {
                // Ignore the FPM document as that path won't exist in the reference website
                if path != "FPM/" {
                    format!(
                        "\n<link rel=\"canonical\" href=\"{canonical_base}{path}\" />",
                        canonical_base = url,
                        path = path
                    )
                } else {
                    "".to_string()
                }
            }
            None => "".to_string(),
        }
    }

    /// aliases() returns the list of the available aliases at the package level.
    pub fn aliases(&self) -> fpm::Result<std::collections::BTreeMap<&str, &fpm::Package>> {
        let mut resp = std::collections::BTreeMap::new();
        self.dependencies.iter().for_each(|d| {
            resp.insert(
                d.alias.as_ref().unwrap_or(&d.package.name).as_str(),
                &d.package,
            );
        });
        Ok(resp)
    }

    pub fn dependency_aliases(&self) -> std::collections::BTreeMap<String, String> {
        let mut resp = std::collections::BTreeMap::new();
        self.dependencies.iter().for_each(|dep| {
            resp.extend(dep.aliases.clone());
        });
        resp
    }
}
