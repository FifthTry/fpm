#[tokio::main]
async fn main() {
    let matches = app(authors()).get_matches();

    let config = fpm::Config::read().await.expect("failed to read config");

    if matches.subcommand_matches("build").is_some() {
        fpm::build(&config).await.expect("build failed");
    }
    if let Some(sync) = matches.subcommand_matches("sync") {
        if let Some(source) = sync.values_of("source") {
            let sources = source.map(|v| v.to_string()).collect();
            fpm::sync(&config, Some(sources))
                .await
                .expect("sync failed");
        } else {
            fpm::sync(&config, None).await.expect("sync failed");
        }
    }
    if let Some(status) = matches.subcommand_matches("status") {
        let source = status.value_of("source");
        fpm::status(&config, source).await.expect("status failed");
    }
    if matches.subcommand_matches("diff").is_some() {
        fpm::diff(&config).await.expect("diff failed");
    }
    if let Some(tracks) = matches.subcommand_matches("start-tracking") {
        let source = tracks.value_of("source").unwrap();
        let target = tracks.value_of("target").unwrap();
        fpm::start_tracking(&config, source, target)
            .await
            .expect("start-tracking failed");
    }
    if let Some(mark) = matches.subcommand_matches("mark-upto-date") {
        let source = mark.value_of("source").unwrap();
        let target = mark.value_of("target");
        fpm::mark_upto_date(&config, source, target)
            .await
            .expect("mark-upto-date failed");
    }
    if let Some(mark) = matches.subcommand_matches("stop-tracking") {
        let source = mark.value_of("source").unwrap();
        let target = mark.value_of("target");
        fpm::stop_tracking(&config, source, target)
            .await
            .expect("stop-tracking failed");
    }
}

fn app(authors: &'static str) -> clap::App<'static, 'static> {
    clap::App::new("fpm: FTD Package Manager")
        .version(env!("CARGO_PKG_VERSION"))
        .author(authors)
        .setting(clap::AppSettings::ArgRequiredElseHelp)
        .arg(
            clap::Arg::with_name("verbose")
                .short("v")
                .multiple(true)
                .help("Sets the level of verbosity"),
        )
        .arg(
            clap::Arg::with_name("test")
                .long("--test")
                .help("Runs the command in test mode")
                .hidden(true),
        )
        .subcommand(
            clap::SubCommand::with_name("build")
                .about("Build static site from this fpm package")
                .version(env!("CARGO_PKG_VERSION")),
        )
        .subcommand(
            clap::SubCommand::with_name("sync")
                .arg(clap::Arg::with_name("source").multiple(true))
                .about("Sync with fpm-repo or .history folder if not using fpm-repo")
                .version(env!("CARGO_PKG_VERSION")),
        )
        .subcommand(
            clap::SubCommand::with_name("status")
                .arg(clap::Arg::with_name("source"))
                .about("Show the status of files in this fpm package")
                .version(env!("CARGO_PKG_VERSION")),
        )
        .subcommand(
            clap::SubCommand::with_name("diff")
                .about("Show un-synced changes to files in this fpm package")
                .version(env!("CARGO_PKG_VERSION")),
        )
        .subcommand(
            clap::SubCommand::with_name("check")
                .about("Check if everything is fine with current fpm package")
                .version(env!("CARGO_PKG_VERSION")),
        )
        .subcommand(
            clap::SubCommand::with_name("mark-upto-date")
                .args(&[
                    clap::Arg::with_name("source").required(true),
                    clap::Arg::with_name("target")
                        .long("--target")
                        .takes_value(true),
                ])
                .about("Marks file as up to date.")
                .version(env!("CARGO_PKG_VERSION")),
        )
        .subcommand(
            clap::SubCommand::with_name("start-tracking")
                .args(&[
                    clap::Arg::with_name("source").required(true),
                    clap::Arg::with_name("target")
                        .long("--target")
                        .takes_value(true)
                        .required(true),
                ])
                .about("Add a tracking relation between two files")
                .version(env!("CARGO_PKG_VERSION")),
        )
        .subcommand(
            clap::SubCommand::with_name("stop-tracking")
                .args(&[
                    clap::Arg::with_name("source").required(true),
                    clap::Arg::with_name("target")
                        .long("--target")
                        .takes_value(true),
                ])
                .about("Remove a tracking relation between two files")
                .version(env!("CARGO_PKG_VERSION")),
        )
}

pub fn authors() -> &'static str {
    Box::leak(
        env!("CARGO_PKG_AUTHORS")
            .split(':')
            .map(|v| v.split_once('<').map(|(v, _)| v.trim()).unwrap_or_default())
            .collect::<Vec<_>>()
            .join(", ")
            .into_boxed_str(),
    )
}
