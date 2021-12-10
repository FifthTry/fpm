#[derive(Debug)]
pub struct Config {
    pub package: fpm::Package,
    pub root: String,
    pub fonts: Vec<fpm::Font>,
    pub dependencies: Vec<fpm::Dependency>,
    pub ignored: ignore::overrides::Override,
}

impl Config {
    pub fn get_font_style(&self) -> String {
        let generated_style = self
            .fonts
            .iter()
            .fold("".to_string(), |c, f| format!("{}\n{}", c, f.to_html()));
        return match generated_style.is_empty() {
            false => format!("<style>{}</style>", generated_style),
            _ => format!(""),
        };
    }

    pub async fn read() -> fpm::Result<Config> {
        let root_dir = std::env::current_dir()
            .expect("Panic1")
            .to_str()
            .expect("panic")
            .to_string();
        let (_, package_folder_name) = root_dir.as_str().rsplit_once("/").expect("");
        let (_is_okay, base_dir) = find_fpm_file(root_dir.clone());

        let lib = fpm::Library::default();
        let id = "fpm".to_string();
        let doc = std::fs::read_to_string(format!("{}/FPM.ftd", base_dir.as_str()))
            .unwrap_or_else(|_| panic!("cant read file. {}/FPM.ftd", base_dir.as_str()));
        let b = match ftd::p2::Document::from(id.as_str(), doc.as_str(), &lib) {
            Ok(v) => v,
            Err(e) => {
                return Err(fpm::Error::ConfigurationError {
                    message: format!("failed to parse {}: {:?}", id, &e),
                });
            }
        };
        let package: fpm::Package = b.get("fpm#package")?;
        let dep: Vec<fpm::Dependency> = b.get("fpm#dependency")?;
        let fonts: Vec<fpm::Font> = b.get("fpm#font")?;

        if package_folder_name != package.name {
            return Err(fpm::Error::ConfigurationError {
                message: "package name and folder name must match".to_string(),
            });
        }

        let ignored = {
            let mut overrides = ignore::overrides::OverrideBuilder::new("./");
            for ig in b.get::<Vec<String>>("fpm#ignore")? {
                if let Err(e) = overrides.add(format!("!{}", ig.as_str()).as_str()) {
                    return Err(fpm::Error::ConfigurationError {
                        message: format!("failed parse fpm.ignore: {} => {:?}", ig, e),
                    });
                }
            }

            match overrides.build() {
                Ok(v) => v,
                Err(e) => {
                    return Err(fpm::Error::ConfigurationError {
                        message: format!("failed parse fpm.ignore: {:?}", e),
                    });
                }
            }
        };

        let c = Config {
            package,
            root: base_dir,
            fonts,
            dependencies: dep.to_vec(),
            ignored,
        };
        fpm::ensure_dependencies(dep).await?;

        Ok(c)
    }
}

fn find_fpm_file(dir: String) -> (bool, String) {
    if std::path::Path::new(format!("{}/FPM.ftd", dir).as_str()).exists() {
        (true, dir)
    } else {
        if let Some((parent_dir, _)) = dir.rsplit_once("/") {
            return find_fpm_file(parent_dir.to_string());
        };
        (false, "".to_string())
    }
}

#[derive(serde::Deserialize, Debug)]
pub struct Package {
    pub name: String,
    pub about: Option<String>,
    pub domain: Option<String>,
}
