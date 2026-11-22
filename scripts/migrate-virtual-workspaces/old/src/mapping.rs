// The mapping: what `plan` proposes and the user may edit, line by line, and what `apply`
// reads. One line per decision; `?` marks one the user fills in, `#` starts a comment.
// Quoted words are JSON strings.
//
//   folder "<path>" = register | into <project> | none
//   record <n> <project> = name "<name>", open | closed
//   label "<label>" <project> = name "<name>", open | closed
//   message <id> = <project>
//   intent <id> = <project>
//
// A project is named by its id as the old store has it (`p2`) or by its folder. Each key (a
// folder, a record or label with its project, a message, an intent) is given once: a key
// given twice is refused, naming both lines, so no line decides in place of another.
use std::collections::BTreeMap;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Key {
    Record(usize),
    Label(String),
}

#[derive(Clone, PartialEq, Debug)]
pub enum Folder {
    Register,
    Into(String),
    None,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Ws {
    pub name: String,
    pub open: bool,
}

/// The mapping as read: projects are still as written (an id or a folder).
#[derive(Default, Debug)]
pub struct Mapping {
    pub folders: BTreeMap<String, Folder>,
    /// Each workspace: its record or label, and its project.
    pub workspaces: BTreeMap<(Key, String), Ws>,
    /// `message <id>`, `intent <id>`: the project of the workspace it goes to.
    pub settled: BTreeMap<String, String>,
    /// Lines still holding `?`.
    pub open: Vec<String>,
    /// The line (from 1) each workspace is given on.
    pub lines: BTreeMap<(Key, String), usize>,
}

fn tokens(line: &str) -> Result<Vec<String>, String> {
    let mut out = vec![];
    let mut rest = line.trim();
    while !rest.is_empty() {
        let c = rest.chars().next().unwrap();
        if c == '#' {
            break;
        }
        if c.is_whitespace() {
            rest = rest.trim_start();
            continue;
        }
        if c == '=' || c == ',' {
            out.push(c.to_string());
            rest = &rest[1..];
            continue;
        }
        if c == '"' {
            // A JSON string: find its end, escapes included.
            let mut end = None;
            let mut esc = false;
            for (k, ch) in rest.char_indices().skip(1) {
                match (esc, ch) {
                    (true, _) => esc = false,
                    (false, '\\') => esc = true,
                    (false, '"') => {
                        end = Some(k);
                        break;
                    }
                    _ => {}
                }
            }
            let end = end.ok_or("a quote is not closed")?;
            let s: String = serde_json::from_str(&rest[..=end]).map_err(|e| e.to_string())?;
            out.push(format!("\"{s}"));
            rest = &rest[end + 1..];
            continue;
        }
        let end = rest.find(|ch: char| ch.is_whitespace() || ch == '=' || ch == ',' || ch == '#').unwrap_or(rest.len());
        out.push(rest[..end].to_string());
        rest = &rest[end..];
    }
    Ok(out)
}

/// A token as text: a quoted one without its mark.
fn text(t: &str) -> &str {
    t.strip_prefix('"').unwrap_or(t)
}

pub fn parse(src: &str) -> Result<Mapping, String> {
    let mut m = Mapping::default();
    let mut given: BTreeMap<String, usize> = BTreeMap::new();
    for (n, line) in src.lines().enumerate() {
        let t = tokens(line).map_err(|e| format!("mapping line {}: {e}", n + 1))?;
        if t.is_empty() {
            continue;
        }
        let bad = |why: &str| format!("mapping line {}: {why}: {}", n + 1, line.trim());
        // The key, as written, before anything else: a line still open counts too.
        if let Some(eq) = t.iter().position(|x| x == "=")
            && !t[..eq].iter().any(|x| x == "?")
        {
            let key = t[..eq].iter().map(|x| if x.starts_with('"') { quoted(text(x)) } else { x.clone() }).collect::<Vec<_>>().join(" ");
            if let Some(first) = given.insert(key.clone(), n + 1) {
                return Err(format!("mapping lines {first} and {} both give {key}: give it once", n + 1));
            }
        }
        if t.iter().any(|x| x == "?") {
            m.open.push(line.trim().to_string());
            continue;
        }
        let eq = t.iter().position(|x| x == "=").ok_or_else(|| bad("no ="))?;
        let (lhs, rhs) = (&t[..eq], &t[eq + 1..]);
        match lhs.first().map(String::as_str) {
            Some("folder") if lhs.len() == 2 => {
                let f = match rhs.iter().map(|x| text(x)).collect::<Vec<_>>()[..] {
                    ["register"] => Folder::Register,
                    ["none"] => Folder::None,
                    ["into", p] => Folder::Into(p.to_string()),
                    _ => return Err(bad("a folder is register, into <project> or none")),
                };
                m.folders.insert(text(&lhs[1]).to_string(), f);
            }
            Some(w @ ("record" | "label")) if lhs.len() == 3 => {
                let key = if w == "record" { Key::Record(lhs[1].parse().map_err(|_| bad("a record is named by its number"))?) } else { Key::Label(text(&lhs[1]).to_string()) };
                let mut ws = Ws { name: String::new(), open: true };
                let mut named = false;
                for part in rhs.split(|x| x == ",") {
                    match part.iter().map(|x| text(x)).collect::<Vec<_>>()[..] {
                        ["name", s] => {
                            ws.name = s.to_string();
                            named = true;
                        }
                        ["open"] => ws.open = true,
                        ["closed"] => ws.open = false,
                        _ => return Err(bad("a workspace is: name \"...\", open or closed")),
                    }
                }
                if !named || ws.name.trim().is_empty() {
                    return Err(bad("a workspace needs a name"));
                }
                let key = (key, text(&lhs[2]).to_string());
                m.lines.insert(key.clone(), n + 1);
                m.workspaces.insert(key, ws);
            }
            Some(w @ ("message" | "intent")) if lhs.len() == 2 && rhs.len() == 1 => {
                m.settled.insert(format!("{w} {}", lhs[1]), text(&rhs[0]).to_string());
            }
            _ => return Err(bad("not a line of the mapping")),
        }
    }
    Ok(m)
}

/// A word as the mapping writes it: bare when it can be, else a JSON string.
pub fn word(s: &str) -> String {
    if !s.is_empty() && !s.contains(|c: char| c.is_whitespace() || matches!(c, '"' | '=' | ',' | '#' | '?' | '\\')) {
        s.to_string()
    } else {
        serde_json::to_string(s).unwrap()
    }
}

pub fn quoted(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}
