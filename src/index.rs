use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::{Arc, RwLock};

use git2::{DiffDelta, DiffHunk, DiffLine, DiffOptions, Repository};
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::helper::CrateReq;
use crate::easy_git::EasyGit;

use std::env;
use std::io::Write;

lazy_static! {
    static ref UPSTREAM: &'static str = Box::leak(
        env::var("UPSTREAM")
            .unwrap_or_else(|_| "https://github.com/rust-lang/crates.io-index.git".to_string())
            .into_boxed_str()
    );
}

#[derive(Clone)]
pub struct GitIndex {
    repo: Arc<Repository>,
}

unsafe impl Send for GitIndex {}

/// crates-io config.json
///
/// Default Config
/// ```json
/// {
///     "dl": "https://crates.io/api/v1/crates",
///     "api": "https://crates.io"
/// }
/// ```
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub dl: String,
    pub api: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            dl: "https://crates.io/api/v1/crates".to_string(),
            api: "https://crates.io".to_string(),
        }
    }
}

impl GitIndex {
    pub fn new<P: AsRef<Path>>(path: P, config: &Config) -> Result<Self, Error> {
        let repo = Repository::open(&path).or_else(|_| Repository::clone(&UPSTREAM, &path))?;
        let config_file = File::open(path.as_ref().join("config.json"))?;
        let local_config: Config = serde_json::from_reader(config_file)?;
        if local_config != *config {
            repo.reset_origin_hard()?;
            {
                debug!("{:?}", path.as_ref().join("config.json"));
                let mut config_file = OpenOptions::new()
                    .truncate(true)
                    .write(true)
                    .create(true)
                    .open(path.as_ref().join("config.json"))?;
                config_file.write_all(serde_json::to_string_pretty(config)?.as_bytes())?;
                config_file.write_all(&[b'\n'])?;
            }
            repo.commit_message("Add mirror", &repo.add("config.json")?)?;
        }
        Ok(GitIndex {
            repo: Arc::new(repo),
        })
    }

    pub fn update(&self) -> Result<Vec<CrateReq>, Error> {
        self.repo.fetch_origin()?;
        let crates = self.diff("HEAD~1", "origin/HEAD")?;
        self.repo.rebase_master()?;
        Ok(crates)
    }

    fn diff<A, B>(&self, a: A, b: B) -> Result<Vec<CrateReq>, Error>
    where
        A: AsRef<str>,
        B: AsRef<str>,
    {
        let head = self.repo.revparse_single(a.as_ref())?;
        let origin = self.repo.revparse_single(b.as_ref())?;
        debug!("diff from {} to {}", a.as_ref(), b.as_ref());
        let mut diff_opts = DiffOptions::new();
        diff_opts
            .force_text(true)
            .ignore_case(true)
            .ignore_filemode(true)
            .context_lines(0);

        let diff = self.repo.diff_tree_to_tree(
            Some(&head.peel_to_tree()?),
            Some(&origin.peel_to_tree()?),
            Some(&mut diff_opts),
        )?;
        let lines = RwLock::new(Vec::new());
        let mut file_cb = |_: DiffDelta, _: f32| -> bool { true };
        let mut line_cb = |_: DiffDelta, _: Option<DiffHunk>, line: DiffLine| -> bool {
            if line.old_lineno().is_none() && line.origin() == '+' {
                let line = std::str::from_utf8(line.content()).map(|s| s.to_owned());
                lines.write().unwrap().push(line);
            }
            true
        };
        diff.foreach(&mut file_cb, None, None, Some(&mut line_cb))?;
        let crates: Vec<CrateReq> = lines
            .into_inner()
            .unwrap()
            .iter()
            .filter_map(|r| r.as_ref().ok())
            .filter_map(|l| serde_json::from_str(l.as_str()).ok())
            .collect();
        Ok(crates)
    }
}

#[test]
fn test() {
    log4rs::init_file("config/log4rs.yml", Default::default()).unwrap();
    let gi = GitIndex::new(
        "index",
        &Config {
            dl: "https://crates-static.project5e.com/{crate}/{version}".to_string(),
            ..Default::default()
        },
    )
    .unwrap();
    // debug!("{:?}", gi.head_author());
    // let diff = gi.update().unwrap();
    // debug!("{:?}", diff);
    let crates = gi.update().unwrap();
    for krate in crates {
        debug!("{:?}", krate);
    }
}
