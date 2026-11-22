// `new write` and `new check`: see lib.rs.
use std::path::Path;

use migrate_virtual_workspaces_new::{check, write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let run = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["write", dir, out] => write::write(Path::new(dir), Path::new(out)),
        ["check", dir, out] => check::check(Path::new(dir), Path::new(out)),
        _ => Err("usage: new write <dir> <out.db> | new check <dir> <out.db>".into()),
    };
    match run {
        Ok(out) => print!("{out}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
