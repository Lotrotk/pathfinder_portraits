use std::path::PathBuf;

const DOWNLOADS_ARG: &str = "downloads";
const PORTRAITS_ARG: &str = "portraits";
const PREFIX_ARG: &str = "prefix";
const KEEP_ORIGINAL_PATH_ARG: &str = "keep-original-path";
const REMOVE_USELESS_DIRS_ARG: &str = "remove-useless-dirs";
const REMOVE_DUPLICATE_DIRS_ARG: &str = "remove-duplicate-dirs";

#[derive(Debug)]
pub struct Args {
    pub downloads_dir: PathBuf,
    pub portraits_dir: PathBuf,
    pub prefix: String,
    pub keep_original_path: bool,
    pub remove_useless_dirs: bool,
    pub remove_duplicate_dirs: bool,
}

impl Args {
    pub fn fetch() -> Self {
        let matches = get_matches();
        let downloads_dir = matches.get_one::<PathBuf>(DOWNLOADS_ARG).unwrap().clone();
        let portraits_dir = matches.get_one::<PathBuf>(PORTRAITS_ARG).unwrap().clone();
        let prefix = matches.get_one::<String>(PREFIX_ARG).unwrap().clone();
        let keep_original_path = matches.get_flag(KEEP_ORIGINAL_PATH_ARG);
        let remove_useless_dirs = matches.get_flag(REMOVE_USELESS_DIRS_ARG);
        let remove_duplicate_dirs = matches.get_flag(REMOVE_DUPLICATE_DIRS_ARG);
        assert_is_dir(&downloads_dir);
        assert_is_dir(&portraits_dir);
        Self {
            downloads_dir,
            portraits_dir,
            prefix,
            keep_original_path,
            remove_useless_dirs,
            remove_duplicate_dirs,
        }
    }
}

fn get_matches() -> clap::ArgMatches {
    let downloads_dir_arg = clap::Arg::new(DOWNLOADS_ARG)
        .required(true)
        .long(DOWNLOADS_ARG)
        .action(clap::ArgAction::Set)
        .value_name("PATH")
        .value_parser(clap::builder::PathBufValueParser::new())
        .help(
            r#"The path where the portraits directory structure is located.
The contents will be moved to the Portraits directory.
This path may equal that of the Portraits directory."#,
        );
    let portraits_dir_arg = clap::Arg::new(PORTRAITS_ARG)
        .required(true)
        .long(PORTRAITS_ARG)
        .action(clap::ArgAction::Set)
        .value_name("PATH")
        .value_parser(clap::builder::PathBufValueParser::new())
        .help(r#"The path to the owlcat game's "Portraits" directory"#);
    let prefix_arg = clap::Arg::new(PREFIX_ARG)
        .required(false)
        .long(PREFIX_ARG)
        .action(clap::ArgAction::Set)
        .value_name("PREFIX")
        .value_parser(clap::builder::NonEmptyStringValueParser::new())
        .default_value("pf_portrait_")
        .help(r#"Every directory in the Portraits directory will have this prefix"#);
    let keep_original_path_arg = clap::Arg::new(KEEP_ORIGINAL_PATH_ARG)
        .required(false)
        .long(KEEP_ORIGINAL_PATH_ARG)
        .action(clap::ArgAction::SetTrue)
        .help(r#"Keeping the original path means the program will do a best effort to have the directories in Portraits reflect their original path in the downloads dir."#);
    let remove_useless_dirs_arg = clap::Arg::new(REMOVE_USELESS_DIRS_ARG)
        .required(false)
        .long(REMOVE_USELESS_DIRS_ARG)
        .action(clap::ArgAction::SetTrue)
        .help(r#"Remove all directories in the Portraits directory that do not contain "Small.png", "Medium.png" and "Fulllength.png"."#);
    let remove_duplicate_dirs_arg = clap::Arg::new(REMOVE_DUPLICATE_DIRS_ARG)
        .required(false)
        .long(REMOVE_DUPLICATE_DIRS_ARG)
        .action(clap::ArgAction::SetTrue)
        .help(r#"Remove all directories in the downloads directory whose "Small.png", "Medium.png" and "Fulllength.png" match that of another."#);
    clap::Command::new("Portraits")
        .before_help(r#"This program is intended to use with Owlcat's Pathfinder games Custom Portraits.
As a first step, you must unpack all custom portraits into a directory structure (downloads dir).
Next, this program will recursively scan the contents of that directory structure for directories which contain "Small.png", "Medium.png" and "Fulllength.png".
Then, it will move those directories into the Portraits directory (portraits dir)."#)
        .version(env!("CARGO_PKG_VERSION"))
        .arg(downloads_dir_arg)
        .arg(portraits_dir_arg)
        .arg(prefix_arg)
        .arg(keep_original_path_arg)
        .arg(remove_useless_dirs_arg)
        .arg(remove_duplicate_dirs_arg)
        .get_matches()
}

fn assert_is_dir(path: &std::path::Path) {
    if path.is_dir() {
        return;
    }
    panic!("\"{}\" does not point to a directory", path.display());
}
