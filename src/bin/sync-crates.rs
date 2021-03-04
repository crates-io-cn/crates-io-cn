use clap::{App, Arg};
use directories::UserDirs;
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{self, exit}
};

const NAME: &str = ".crates-io";

struct LockGuard(PathBuf);

fn main() {
    let user_dirs = UserDirs::new().expect("cannot locate user directories");
    let default_path = user_dirs.home_dir().join(NAME);
    if !default_path.exists() {
        fs::create_dir_all(&default_path).expect("unable to create tool directory");
    }
    if let Err(e) = get_lock(&default_path) {
        eprintln!("cannot obtain lock\nreason: {}", e);
        exit(-1)
    }
    let _guard = LockGuard(default_path.clone());
    let default_db = default_path.join("db");
    let matches = App::new("crates.io sync")
        .arg(
            Arg::with_name("db")
                .short("d")
                .long("database")
                .value_name("DB_PATH")
                .help("the sync db location")
                .default_value_os(default_db.as_os_str())
                .takes_value(true),
        )
        .get_matches();
    let db_path = Path::new(matches.value_of("db").unwrap());
    let conn = sqlite::open(&db_path).expect("cannot open db");
    conn.execute(include_str!("init.sql")).expect("cannot init db");

    println!("{}", matches.value_of("db").unwrap());
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let lock = self.0.join(".lock");
        if lock.exists() {
            if let Ok(pid) = read_pid(&lock) {
                if pid == process::id() {
                    fs::remove_file(&lock).unwrap();
                }
            }
        }
    }
}

fn read_pid<P: AsRef<Path>>(lock_path: P) -> io::Result<u32> {
    let mut pid_read = String::new();
    fs::File::open(lock_path.as_ref()).and_then(|mut f| f.read_to_string(&mut pid_read))?;
    return Ok(pid_read.parse().unwrap())
}

fn get_lock(default_path: &PathBuf) -> io::Result<()> {
    let lock = default_path.join(".lock");
    if lock.exists() {
        let result = read_pid(&lock);
        return match result {
            Ok(pid) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("lock is hold by pid {}", pid),
            )),
            Err(_) => Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "lock is hold by other process",
            )),
        };
    }
    let pid = process::id();
    {
        let mut lock_file = fs::File::create(&lock)?;
        write!(&mut lock_file, "{}", pid)?;
    }
    let pid_read = read_pid(&lock)?;
    return if pid == pid_read {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("cannot lock, expect pid {}, recheck with pid {}", pid, pid_read)
        ))
    }
}
