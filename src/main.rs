use anyhow::anyhow;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

mod args;

const MAX_ATTEMPTS_WHEN_NO_NEED_TO_KEEP_ORIGINAL_FILENAME: u32 = 1000000;

trait ScanDir {
    fn include(&self, path: &Path) -> bool;
}

struct PortraitDir;

impl ScanDir for PortraitDir {
    fn include(&self, path: &Path) -> bool {
        path.join("Small.png").exists()
            && path.join("Medium.png").exists()
            && path.join("Fulllength.png").exists()
    }
}

struct NonPortraitDir;

impl ScanDir for NonPortraitDir {
    fn include(&self, path: &Path) -> bool {
        !PortraitDir {}.include(path)
    }
}

#[derive(Debug)]
struct Scan<'a, T>
where
    T: ScanDir,
{
    root: &'a Path,
    dirs: Vec<PathBuf>,
    scan_dir: T,
}

impl<'a, T> Scan<'a, T>
where
    T: ScanDir,
{
    pub fn new(root: &'a Path, scan_dir: T) -> Self {
        let dirs = Vec::new();
        let mut scan = Self {
            root,
            dirs,
            scan_dir,
        };
        scan.scan_dir(root);
        scan
    }

    fn scan_dir(&mut self, dir: &Path) {
        let mut dirs_to_scan = Vec::new();
        let contents = match std::fs::read_dir(dir) {
            Ok(contents) => contents,
            Err(_) => {
                eprintln!("Failed to scan the contents of {}", dir.display());
                return;
            }
        };
        for dir in contents
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
        {
            let path = dir.path();
            if self.scan_dir.include(&path) {
                self.dirs.push(path.clone());
            }
            dirs_to_scan.push(path);
        }
        for dir in dirs_to_scan {
            self.scan_dir(&dir)
        }
    }

    pub fn size(&self) -> usize {
        self.dirs.len()
    }

    pub fn erase(self) {
        for dir in &self.dirs {
            if std::fs::remove_dir_all(dir).is_err() {
                eprintln!("Failed to erase {}", dir.display());
            }
        }
    }
}

#[derive(Clone, Copy)]
struct OriginalFileName<'a> {
    dir_components: &'a [&'a OsStr],
    file_name: &'a OsStr,
}

struct Move<'a: 'b, 'b> {
    scan: &'a Scan<'b, PortraitDir>,
    output: Vec<Option<PathBuf>>,
}

impl<'a: 'b, 'b> Move<'a, 'b> {
    pub fn new(
        scan: &'a Scan<'b, PortraitDir>,
        target: &Path,
        dir_prefix: &str,
        keep_original_path: bool,
    ) -> anyhow::Result<Self> {
        if !target.is_dir() {
            return Err(anyhow!("{} is not a directory", target.display()));
        }
        let mut output: Vec<Option<PathBuf>> = Vec::new();
        let mut output_set: HashSet<PathBuf> = HashSet::new();

        let scan_skip_components = scan.root.components().count();
        for dir in &scan.dirs {
            let original_filename = {
                if keep_original_path {
                    let mut dir_components: Vec<&OsStr> = dir
                        .components()
                        .skip(scan_skip_components)
                        .map(std::path::Component::as_os_str)
                        .collect();
                    let file_name = match dir_components.pop() {
                        Some(file_name) => Self::stripped(file_name, dir_prefix),
                        None => {
                            output.push(None);
                            continue;
                        }
                    };
                    Some((dir_components, file_name))
                } else {
                    None
                }
            };
            let original_filename =
                original_filename
                    .as_ref()
                    .map(|(dir_components, file_name)| {
                        let dir_components = &dir_components[..];
                        let file_name = file_name.as_os_str();
                        OriginalFileName {
                            dir_components,
                            file_name,
                        }
                    });
            let max_attempts: u32 = if original_filename.is_some() {
                1000
            } else {
                MAX_ATTEMPTS_WHEN_NO_NEED_TO_KEEP_ORIGINAL_FILENAME
            };
            let mut attempt: u32 = 0;
            let mut rename = Self::rename(target, dir_prefix, attempt, original_filename);
            output.push(loop {
                let r = &rename;
                if !(output_set.contains(r) || r.exists()) {
                    output_set.insert(rename.clone());
                    break Some(rename);
                }
                attempt += 1;
                if attempt >= max_attempts {
                    break None;
                }
                rename = Self::rename(target, dir_prefix, attempt, original_filename);
            });
        }
        Ok(Self { scan, output })
    }

    fn stripped(file_name: &OsStr, dir_prefix: &str) -> OsString {
        let base_bytes: &[u8] = Path::new(dir_prefix).as_os_str().as_encoded_bytes();
        let mut bytes: &[u8] = file_name.as_encoded_bytes();
        while bytes.len() >= base_bytes.len() && &bytes[..base_bytes.len()] == base_bytes {
            bytes = &bytes[base_bytes.len()..];
        }
        unsafe { OsString::from_encoded_bytes_unchecked(bytes.to_vec()) }
    }

    fn rename(
        target: &Path,
        dir_prefix: &str,
        attempt: u32,
        original_filename: Option<OriginalFileName<'_>>,
    ) -> PathBuf {
        let mut new_filename = OsString::new();
        new_filename.push(dir_prefix);
        if let Some(OriginalFileName {
            dir_components,
            file_name,
        }) = original_filename
        {
            for component in dir_components.iter() {
                new_filename.push(component);
                new_filename.push("_");
            }
            new_filename.push(file_name);
            if attempt > 0 {
                new_filename.push(format!("_{:03}", attempt));
            }
        } else {
            assert_eq!(MAX_ATTEMPTS_WHEN_NO_NEED_TO_KEEP_ORIGINAL_FILENAME, 1000000);
            new_filename.push(format!("{:06}", attempt));
        }
        target.join(new_filename)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Path, Option<&Path>)> {
        self.scan.dirs.iter().map(PathBuf::as_path).zip(
            self.output
                .iter()
                .map(|output| output.as_ref().map(PathBuf::as_path)),
        )
    }
}

fn run(args: &args::Args) -> anyhow::Result<()> {
    let args::Args {
        downloads_dir,
        portraits_dir,
        prefix,
        keep_original_path,
        remove_useless_dirs: _,
    } = args;
    let scan = Scan::new(downloads_dir, PortraitDir);
    let mv = Move::new(&scan, portraits_dir, prefix, *keep_original_path)?;
    let mut success: usize = 0;
    let mut failure: usize = 0;
    for (src, dst) in mv.iter() {
        if let Some(dst) = dst {
            if std::fs::rename(src, dst).is_ok() {
                success += 1;
            } else {
                failure += 1;
                eprintln!("Unable to rename {} to {}", src.display(), dst.display());
            }
        } else {
            failure += 1;
            eprintln!("Unable to rename {}", src.display());
        }
    }
    println!(
        "Found {} entries, {} succeeded, {} failed",
        scan.size(),
        success,
        failure
    );
    Ok(())
}

fn cleanup(args: &args::Args) {
    let args::Args {
        downloads_dir: _,
        portraits_dir,
        prefix: _,
        keep_original_path: _,
        remove_useless_dirs,
    } = args;
    if !remove_useless_dirs {
        return;
    }
    let scan = Scan::new(portraits_dir, NonPortraitDir);
    scan.erase();
}

fn main() -> anyhow::Result<()> {
    let args = args::Args::fetch();
    run(&args)?;
    cleanup(&args);
    Ok(())
}
