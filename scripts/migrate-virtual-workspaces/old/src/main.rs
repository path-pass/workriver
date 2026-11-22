// `old plan` and `old apply`: see lib.rs.
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let run = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["plan", copy, map] => migrate_virtual_workspaces_old::plan(Path::new(copy), Path::new(map)),
        ["apply", copy, map, dir] => migrate_virtual_workspaces_old::apply(Path::new(copy), Path::new(map), Path::new(dir)),
        _ => Err("usage: old plan <copy.db> <mapping> | old apply <copy.db> <mapping> <dir>".into()),
    };
    match run {
        Ok(out) => print!("{out}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
