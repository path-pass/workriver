// A repository's files at HEAD as a tree of folders, with line counts.
use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::*;

/// A folder or file of the tree, as the UI's `ModuleNode`.
#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct Module {
    pub name: String,
    pub path: String,
    pub loc: u64,
    pub files: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Module>>,
}

/// The repository's files at HEAD as a tree of folders, with line counts.
pub fn tree(at: &Runner, repo: &Path) -> Result<Module> {
    let top = toplevel(at, repo)?;
    let name = top.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut root = Module { name, children: Some(vec![]), ..Default::default() };
    if !has_rev(at, &top, "HEAD") {
        return Ok(root);
    }
    // `git grep -c ''` counts lines of every text file at HEAD in one go: "HEAD:path\0count".
    let mut loc = HashMap::new();
    for line in String::from_utf8_lossy(&run(at, &top, &["grep", "-I", "-z", "-c", "", "HEAD", "--"], &[1])?).lines() {
        if let Some((path, n)) = line.split_once('\0') {
            loc.insert(path.trim_start_matches("HEAD:").to_string(), n.parse().unwrap_or(0));
        }
    }
    for path in split0(&run(at, &top, &["ls-tree", "-r", "--name-only", "-z", "HEAD"], &[])?) {
        let n = loc.get(&path).copied().unwrap_or(0);
        insert(&mut root, &path, n);
    }
    Ok(root)
}

fn insert(root: &mut Module, path: &str, loc: u64) {
    let mut node = root;
    let mut at = 0;
    for part in path.split('/') {
        node.loc += loc;
        node.files += 1;
        at += part.len() + 1;
        let here = &path[..at - 1];
        let kids = node.children.get_or_insert_with(Vec::new);
        let k = match kids.iter().position(|c| c.name == part) {
            Some(k) => k,
            None => {
                let folder = here.len() < path.len();
                kids.push(Module { name: part.into(), path: here.into(), children: folder.then(Vec::new), ..Default::default() });
                kids.len() - 1
            }
        };
        node = &mut kids[k];
    }
    node.loc += loc;
    node.files += 1;
}
