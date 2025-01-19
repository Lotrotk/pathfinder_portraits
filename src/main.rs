use anyhow::anyhow;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};

mod args;

const MAX_ATTEMPTS_WHEN_NEED_TO_KEEP_ORIGINAL_FILENAME: u32 = 1000;
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

#[derive(Eq, Hash, PartialEq)]
struct Checksum {
    small: md5::Digest,
    medium: md5::Digest,
    full: md5::Digest,
}

impl Checksum {
    pub fn from_dir(dir: &Path) -> Option<Self> {
        let small = Self::check_file(&dir.join("Small.png"))?;
        let medium = Self::check_file(&dir.join("Medium.png"))?;
        let full = Self::check_file(&dir.join("Fulllength.png"))?;
        Some(Self {
            small,
            medium,
            full,
        })
    }

    fn check_file(file: &Path) -> Option<md5::Digest> {
        let mut file = std::fs::File::open(file).ok()?;
        let mut buffer = Vec::new();
        let _ = file.read_to_end(&mut buffer).ok()?;
        Some(md5::compute(&buffer))
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
}

impl Scan<'_, NonPortraitDir> {
    pub fn erase(self) -> usize {
        let mut erased = 0;
        for dir in &self.dirs {
            if std::fs::remove_dir_all(dir).is_err() {
                eprintln!("Failed to erase {}", dir.display());
            } else {
                erased += 1;
            }
        }
        erased
    }
}

impl Scan<'_, PortraitDir> {
    pub fn erase_duplicates(&mut self) -> usize {
        let mut checksums: HashSet<Checksum> = HashSet::new();
        let mut erased = 0;
        self.dirs.retain(|dir| {
            let dir = dir.as_path();
            let checksum = match Checksum::from_dir(dir) {
                Some(checksum) => checksum,
                None => {
                    eprintln!("Failed to get checksum for {}", dir.display());
                    return true;
                }
            };
            if checksums.contains(&checksum) {
                if std::fs::remove_dir_all(dir).is_err() {
                    eprintln!("Failed to erase duplicate {}", dir.display());
                } else {
                    erased += 1;
                }
                false
            } else {
                checksums.insert(checksum);
                true
            }
        });
        erased
    }
}

struct OriginalFileName<'a> {
    dir_components: Vec<&'a OsStr>,
    file_name: OsString,
}

impl<'a> OriginalFileName<'a> {
    pub fn new(dir_prefix: &str, scan_skip_components: usize, dir: &'a Path) -> Option<Self> {
        let mut dir_components: Vec<&OsStr> = dir
            .components()
            .skip(scan_skip_components)
            .map(std::path::Component::as_os_str)
            .collect();
        let file_name = dir_components.pop()?;
        let file_name = Self::stripped(file_name, dir_prefix);
        Some(Self {
            dir_components,
            file_name,
        })
    }

    fn stripped(file_name: &OsStr, dir_prefix: &str) -> OsString {
        let base_bytes: &[u8] = Path::new(dir_prefix).as_os_str().as_encoded_bytes();
        let mut bytes: &[u8] = file_name.as_encoded_bytes();
        while bytes.len() >= base_bytes.len() && &bytes[..base_bytes.len()] == base_bytes {
            bytes = &bytes[base_bytes.len()..];
        }
        unsafe { OsString::from_encoded_bytes_unchecked(bytes.to_vec()) }
    }

    pub fn as_ref(&'a self) -> OriginalFileNameRef<'a> {
        let Self {
            dir_components,
            file_name,
        } = self;
        let dir_components = &dir_components[..];
        let file_name = file_name.as_os_str();
        OriginalFileNameRef {
            dir_components,
            file_name,
        }
    }
}

#[derive(Clone, Copy)]
struct OriginalFileNameRef<'a> {
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
            let (original_filename, max_attempts) = {
                if keep_original_path {
                    let original_filename =
                        OriginalFileName::new(dir_prefix, scan_skip_components, dir);
                    if original_filename.is_none() {
                        output.push(None);
                        continue;
                    }
                    (
                        original_filename,
                        MAX_ATTEMPTS_WHEN_NEED_TO_KEEP_ORIGINAL_FILENAME,
                    )
                } else {
                    (None, MAX_ATTEMPTS_WHEN_NO_NEED_TO_KEEP_ORIGINAL_FILENAME)
                }
            };
            let original_filename = original_filename.as_ref().map(OriginalFileName::as_ref);
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

    fn rename(
        target: &Path,
        dir_prefix: &str,
        attempt: u32,
        original_filename: Option<OriginalFileNameRef<'_>>,
    ) -> PathBuf {
        let mut new_filename = OsString::new();
        new_filename.push(dir_prefix);
        if let Some(OriginalFileNameRef {
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
                assert_eq!(MAX_ATTEMPTS_WHEN_NEED_TO_KEEP_ORIGINAL_FILENAME, 1000);
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

fn prepare(args: &args::Args) -> (Scan<'_, PortraitDir>, usize) {
    let args::Args {
        downloads_dir,
        portraits_dir: _,
        prefix: _,
        keep_original_path: _,
        remove_useless_dirs: _,
        remove_duplicate_dirs,
    } = args;
    let mut scan = Scan::new(downloads_dir, PortraitDir);
    let erased = if *remove_duplicate_dirs {
        scan.erase_duplicates()
    } else {
        0
    };
    (scan, erased)
}

fn run(args: &args::Args, scan: Scan<'_, PortraitDir>) -> anyhow::Result<(usize, usize)> {
    let args::Args {
        downloads_dir: _,
        portraits_dir,
        prefix,
        keep_original_path,
        remove_useless_dirs: _,
        remove_duplicate_dirs: _,
    } = args;
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
    Ok((success, failure))
}

fn cleanup(args: &args::Args) -> usize {
    let args::Args {
        downloads_dir: _,
        portraits_dir,
        prefix: _,
        keep_original_path: _,
        remove_useless_dirs,
        remove_duplicate_dirs: _,
    } = args;
    if !remove_useless_dirs {
        return 0;
    }
    let scan = Scan::new(portraits_dir, NonPortraitDir);
    scan.erase()
}

fn main() -> anyhow::Result<()> {
    let args = args::Args::fetch();
    let (scan, erased_duplicates) = prepare(&args);
    let (success, failure) = run(&args, scan).unwrap_or_else(|err| {
        eprintln!("{}", err);
        (0, 0)
    });
    let erased_useless = cleanup(&args);
    println!(
        r#"Done!
Sucessesfully renamed = {}
Failed to rename      = {}
Erased useless dirs   = {}
Erased duplicate dirs = {}"#,
        success, failure, erased_useless, erased_duplicates
    );
    Ok(())
}
