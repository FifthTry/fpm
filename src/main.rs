fn main() {
    let matches = clap::App::new("FTD Package Manager")
        .version(env!("CARGO_PKG_VERSION"))
        .author("Shobhit S. <shobhit@fifthtry.com>")
        .about("Description...")
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
                .about("Builds the current directory")
                .version(env!("CARGO_PKG_VERSION")),
        )
        .subcommand(
            clap::SubCommand::with_name("sync")
                .about("`sync` with `fpm-repo` or `.history` folder if not using `fpm-repo`")
                .version(env!("CARGO_PKG_VERSION")),
        )
        .subcommand(
            clap::SubCommand::with_name("check")
                .about("Checks the folder structure of the current .FPM.ftd file")
                .version(env!("CARGO_PKG_VERSION")),
        )
        .get_matches();

    if matches.subcommand_matches("build").is_some() {
        fpm::build();
    }
    if matches.subcommand_matches("sync").is_some() {
        fpm::sync();
    }
}
