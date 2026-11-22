// What an agent's terminal shows, in workriver's words: its visible text, a dialog read off
// it (options, the keys its last lines name, the region to show), the keys workriver may
// ever press there, and what of a screen may be kept. Pure: no I/O, no UI text, no harness
// names. The runtime reads the screen and presses the keys (runtime.rs); the service decides.
use serde::{Deserialize, Serialize};

/// A key workriver may press in an agent's terminal: nothing else, and never text. A screen
/// can say anything, and the page has no other way to type.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Up,
    Down,
    Enter,
    Escape,
    Tab,
    /// 1 to 9.
    Digit(u8),
}

impl Key {
    /// The key of that name (`up`, `down`, `enter`, `escape`, `tab`, `1`..`9`); nothing else.
    pub fn named(name: &str) -> Option<Key> {
        Some(match name {
            "up" => Key::Up,
            "down" => Key::Down,
            "enter" => Key::Enter,
            "escape" => Key::Escape,
            "tab" => Key::Tab,
            d if d.len() == 1 && matches!(d.as_bytes()[0], b'1'..=b'9') => Key::Digit(d.as_bytes()[0] - b'0'),
            _ => return None,
        })
    }

    pub fn name(&self) -> String {
        match self {
            Key::Up => "up".into(),
            Key::Down => "down".into(),
            Key::Enter => "enter".into(),
            Key::Escape => "escape".into(),
            Key::Tab => "tab".into(),
            Key::Digit(d) => d.to_string(),
        }
    }
}

impl Serialize for Key {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Key, D::Error> {
        let name = String::deserialize(d)?;
        Key::named(&name).ok_or_else(|| serde::de::Error::custom(format!("not a key workriver presses: {name:?} (up, down, enter, escape, tab, 1-9)")))
    }
}

/// What a place shows now: its visible text, one line per row, with its size.
#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct Screen {
    pub text: String,
    pub cols: u16,
    pub rows: u16,
}

impl Screen {
    /// A screen of this text, its size counted from it.
    pub fn of(text: String) -> Screen {
        let rows = text.lines().count().min(u16::MAX as usize) as u16;
        let cols = text.lines().map(|l| l.chars().count()).max().unwrap_or(0).min(u16::MAX as usize) as u16;
        Screen { text, cols, rows }
    }
}

/// A piece of a terminal's output as the runtime streams it: the first one whole (`full`),
/// then changes; `bytes` are what the terminal draws (its escape sequences included), base64.
#[derive(Serialize, Clone, PartialEq, Debug, Default)]
pub struct Frame {
    pub full: bool,
    pub cols: u16,
    pub rows: u16,
    pub seq: u64,
    pub bytes: String,
}

/// A dialog read off a screen: what is asked, and how to answer.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Dialog {
    /// In their order on the screen.
    pub options: Vec<Item>,
    /// Numbered options (answered by their digit); else a marked list (answered by arrows).
    pub numbered: bool,
    /// The keys its last lines name, each with the words that name it.
    pub hints: Vec<Hint>,
    /// The nearest non-empty line above the first option, as read.
    pub above: Option<String>,
    /// That line when it asks something (`question_line`).
    pub question: Option<String>,
    /// The rows to show: from `top` to `bottom`, both in.
    pub top: usize,
    pub bottom: usize,
    /// The twelve lines above the first option ended before the harness's rule did: there is more
    /// of the dialog above `top`, and it must be seen whole before it is answered.
    pub cut: bool,
}

/// One option of a dialog.
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Item {
    /// Its number (numbered options only).
    pub number: Option<u8>,
    /// Its own line's words, after the number.
    pub words: String,
    /// The lines under it (its wrapped text, notes), trimmed.
    pub more: Vec<String>,
    /// The selection marker is on it.
    pub marked: bool,
    /// The row of its own line.
    pub row: usize,
}

/// A key a dialog's last lines name ("Esc to cancel").
#[derive(Serialize, Clone, PartialEq, Debug)]
pub struct Hint {
    pub key: Key,
    pub words: String,
}

impl Dialog {
    /// Whether a screen the runtime does not call blocked shows a dialog that waits: options
    /// and a key named under them.
    pub fn waiting(&self) -> bool {
        !self.hints.is_empty()
    }

    /// The option the marker is on, by its index.
    pub fn selected(&self) -> Option<usize> {
        self.options.iter().position(|o| o.marked)
    }
}

/// The selection marker before an option.
const MARKERS: [char; 2] = ['❯', '›'];
/// How many lines above the first option are shown at most.
pub const REGION: usize = 12;
/// How many lines may sit under one option.
const UNDER: usize = 6;

fn blank(l: &str) -> bool {
    l.trim().is_empty()
}

/// A full-width rule: a line of one rule character.
fn rule(l: &str, c: char) -> bool {
    let t = l.trim();
    t.chars().count() >= 3 && t.chars().all(|x| x == c)
}

/// A harness's own rule, the top of its dialog: a solid line drawn from column 0 across the
/// screen (one column short of its widest line at most). A rule-like line anywhere else (a
/// command's line, which a dialog indents, or one that stops short) is text an agent may
/// have written, and ends nothing.
fn frame_rule(l: &str, width: usize) -> bool {
    !l.starts_with(char::is_whitespace) && (rule(l, '─') || rule(l, '━')) && l.chars().count() + 1 >= width
}

fn any_rule(l: &str) -> bool {
    ['─', '━', '═', '╌', '┄', '-'].iter().any(|c| rule(l, *c))
}

fn indent(l: &str) -> usize {
    l.chars().take_while(|c| c.is_whitespace()).count()
}

/// A line as an option might read: whether it is marked, the column its words start at
/// (after the marker), and those words; a number when it is `N. words`.
struct Line<'a> {
    marked: bool,
    number: Option<u8>,
    /// Where the number (or, unnumbered, the words) starts.
    at: usize,
    words: &'a str,
}

fn read_line(l: &str) -> Line<'_> {
    let lead = indent(l);
    let rest = l.trim_start();
    let (marked, rest, at) = match rest.chars().next() {
        Some(m) if MARKERS.contains(&m) => {
            let after = &rest[m.len_utf8()..];
            let gap = indent(after);
            (true, after.trim_start(), lead + 1 + gap)
        }
        _ => (false, rest, lead),
    };
    let b = rest.as_bytes();
    let number = (b.len() > 3 && matches!(b[0], b'1'..=b'9') && b[1] == b'.' && b[2] == b' ').then(|| b[0] - b'0');
    let words = if number.is_some() { rest[2..].trim() } else { rest.trim_end() };
    Line { marked, number, at, words }
}

/// The keys one hint line names, each part of it (split at `·`, `|` and ` or `) that starts
/// with one key of the set; any other part (`ctrl+g to edit`, `↑/↓ to navigate`) is text.
fn hints_of(l: &str) -> Vec<Hint> {
    let mut out = vec![];
    for part in l.split(['·', '|']).flat_map(|p| p.split(" or ")) {
        let words = part.trim();
        let first = words.split_whitespace().next().unwrap_or("");
        let first = if first.eq_ignore_ascii_case("press") { words.split_whitespace().nth(1).unwrap_or("") } else { first };
        let key = match first.trim_end_matches([',', ':', '.']).to_lowercase().as_str() {
            "enter" | "return" | "↵" | "⏎" => Key::Enter,
            "esc" | "escape" => Key::Escape,
            "tab" => Key::Tab,
            "↑" | "up" => Key::Up,
            "↓" | "down" => Key::Down,
            _ => continue,
        };
        if !out.iter().any(|h: &Hint| h.key == key) {
            out.push(Hint { key, words: words.to_string() });
        }
    }
    out
}

/// The dialog a screen shows at its bottom, or none. Read from the bottom up, so text above
/// it (a transcript that quotes a dialog, a plan's own numbered steps) is never reached: the
/// lowest non-empty lines that are not options are its hints; above them its options,
/// numbered (`[marker] N. words` counting down to 1, with the lines indented under each, and
/// blank lines and rules between them passed over) or, with no numbers, a run of two or more
/// lines at one indent with one of them marked; above the first option, the lines up to the
/// harness's own rule (`frame_rule`), at most twelve.
pub fn parse(text: &str) -> Option<Dialog> {
    let lines: Vec<&str> = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let mut i = lines.len();
    while i > 0 && blank(lines[i - 1]) {
        i -= 1;
    }
    if i == 0 {
        return None;
    }
    let bottom = i - 1;
    // The hints: the block of lines at the bottom that are not options or rules.
    let mut hints_top = i;
    while hints_top > 0 {
        let l = lines[hints_top - 1];
        if blank(l) || any_rule(l) {
            break;
        }
        let r = read_line(l);
        if r.number.is_some() || r.marked {
            break;
        }
        hints_top -= 1;
    }
    // Rows under a marked row, at its column and naming no key, are the rest of its list.
    if let Some(m) = hints_top.checked_sub(1).map(|k| read_line(lines[k])).filter(|r| r.marked && r.number.is_none()) {
        while hints_top < i && read_line(lines[hints_top]).at == m.at && hints_of(lines[hints_top]).is_empty() {
            hints_top += 1;
        }
    }
    // An option line right at the bottom: no hints.
    let mut hints: Vec<Hint> = vec![];
    for l in &lines[hints_top..i] {
        for h in hints_of(l) {
            if !hints.iter().any(|x| x.key == h.key) {
                hints.push(h);
            }
        }
    }
    let (options, numbered) = numbered_above(&lines, hints_top).map(|o| (o, true)).or_else(|| marked_above(&lines, hints_top).map(|o| (o, false)))?;
    let first = options[0].row;
    // The region: up to the harness's rule above the first option, at most twelve lines.
    let width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let mut top = first;
    let mut cut = false;
    loop {
        if top == 0 {
            break;
        }
        if frame_rule(lines[top - 1], width) {
            break;
        }
        if first - top == REGION {
            cut = true;
            break;
        }
        top -= 1;
    }
    while top < first && blank(lines[top]) {
        top += 1;
    }
    let above = lines[..first].iter().rev().find(|l| !blank(l)).map(|l| l.trim().to_string());
    let question = question_line(above.as_deref());
    Some(Dialog { options, numbered, hints, above, question, top, bottom, cut })
}

/// Numbered options going up from row `from`: the last one first, counting down to 1.
fn numbered_above(lines: &[&str], from: usize) -> Option<Vec<Item>> {
    let mut options: Vec<Item> = vec![];
    let mut under: Vec<(usize, &str)> = vec![];
    let mut want: Option<u8> = None;
    let mut i = from;
    while i > 0 {
        i -= 1;
        let l = lines[i];
        if blank(l) || any_rule(l) {
            continue;
        }
        let r = read_line(l);
        match r.number {
            Some(n) if want.is_none_or(|w| w == n) => {
                // What sat under it must be indented past its number.
                if under.len() > UNDER || under.iter().any(|(_, u)| indent(u) <= r.at) {
                    return None;
                }
                let more = under.drain(..).rev().map(|(_, u)| u.trim().to_string()).collect();
                options.push(Item { number: Some(n), words: r.words.to_string(), more, marked: r.marked, row: i });
                if n == 1 {
                    options.reverse();
                    return Some(options);
                }
                want = Some(n - 1);
            }
            Some(_) => return None,
            None => under.push((i, l)),
        }
    }
    None
}

/// A marked list just above row `from`: two or more lines whose words start at one column,
/// exactly one of them marked.
fn marked_above(lines: &[&str], from: usize) -> Option<Vec<Item>> {
    let mut i = from;
    while i > 0 && blank(lines[i - 1]) {
        i -= 1;
    }
    let mut run: Vec<Item> = vec![];
    let mut col = None;
    while i > 0 {
        let l = lines[i - 1];
        if blank(l) || any_rule(l) {
            break;
        }
        let r = read_line(l);
        if r.number.is_some() || r.words.is_empty() || col.is_some_and(|c| c != r.at) {
            break;
        }
        col = Some(r.at);
        run.push(Item { number: None, words: r.words.to_string(), more: vec![], marked: r.marked, row: i - 1 });
        i -= 1;
    }
    run.reverse();
    (run.len() >= 2 && run.iter().filter(|o| o.marked).count() == 1).then_some(run)
}

/// The row the selection marker is on: the lowest line that starts with one.
pub fn marked_row(text: &str) -> Option<usize> {
    text.split('\n').enumerate().filter(|(_, l)| l.trim_start().starts_with(MARKERS)).map(|(k, _)| k).last()
}

/// The screen with the marker at the start of each line blanked: what stays the same while
/// only the selection moves.
pub fn unmarked(text: &str) -> String {
    text.split('\n')
        .map(|l| {
            let lead = l.len() - l.trim_start().len();
            match l[lead..].chars().next() {
                Some(m) if MARKERS.contains(&m) => format!("{} {}", &l[..lead], &l[lead + m.len_utf8()..]),
                _ => l.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The code points a browser draws as nothing or uses to reorder text: Unicode's
/// Default_Ignorable_Code_Point (17.0, DerivedCoreProperties.txt), which holds every
/// Bidi_Control one. A table of seventeen ranges, not a crate: `icu_properties` has the
/// property, but brings ICU4X's data for it.
const INVISIBLE: [(u32, u32); 17] = [
    (0x00AD, 0x00AD),
    (0x034F, 0x034F),
    (0x061C, 0x061C),
    (0x115F, 0x1160),
    (0x17B4, 0x17B5),
    (0x180B, 0x180F),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x206F),
    (0x3164, 0x3164),
    (0xFE00, 0xFE0F),
    (0xFEFF, 0xFEFF),
    (0xFFA0, 0xFFA0),
    (0xFFF0, 0xFFF8),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0000, 0xE0FFF),
];

/// Screen text as the page may show it: each mark that reorders or hides text becomes a
/// visible `⟨U+202E⟩` (its code point), in its place; nothing is dropped, since dropping
/// also changes what the user approves. Everything else as it was, in its order.
pub fn marked(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let cp = c as u32;
        if INVISIBLE.iter().any(|(a, b)| (*a..=*b).contains(&cp)) {
            out.push_str(&format!("⟨U+{cp:04X}⟩"));
        } else {
            out.push(c);
        }
    }
    out
}

/// What came of pressing keys at a dialog, as the service saw it right after.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// The dialog is gone and the runtime no longer reports the agent waiting.
    Answered,
    /// The same dialog is still there (a key that only moved the selection).
    Open,
    /// Another dialog is there now.
    New,
    /// The dialog is gone, but the runtime still reports the agent waiting.
    Sent,
    /// Something else changed while the selection was being moved: Enter was not sent.
    Changed,
    /// A key may have been sent: its sending failed, or its answer was lost.
    Unsure,
}

/// An option as the record keeps it: its number, and its words cut as `kept`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct KeptOption {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<u8>,
    pub words: String,
}

/// How long a kept question line and a kept option's words may be.
pub const KEPT_LINE: usize = 120;
pub const KEPT_WORDS: usize = 40;

/// What of a screen's line may be kept in the record: cut at the first backtick, `"`, `'`
/// that does not stand between two letters, `$`, `=`, `/`, `~` or `\` (where a command's
/// arguments, an assignment, a quoted value or a path begin), on one line with control
/// characters as spaces, capped at `cap` characters, trimmed and marked.
pub fn kept(line: &str, cap: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut end = chars.len();
    for (k, c) in chars.iter().enumerate() {
        let apostrophe = *c == '\'' && k > 0 && k + 1 < chars.len() && chars[k - 1].is_alphabetic() && chars[k + 1].is_alphabetic();
        if matches!(c, '`' | '"' | '$' | '=' | '/' | '~' | '\\') || (*c == '\'' && !apostrophe) {
            end = k;
            break;
        }
    }
    let one: String = chars[..end].iter().map(|c| if c.is_control() { ' ' } else { *c }).collect();
    let one = one.split(' ').filter(|w| !w.is_empty()).collect::<Vec<_>>().join(" ");
    let capped: String = one.chars().take(cap).collect();
    marked(capped.trim())
}

/// The line above a dialog's first option as it may be kept: only when it asks something
/// (it ends with `?`; a command or a path some dialogs put there does not), cut as `kept`.
pub fn question_line(above: Option<&str>) -> Option<String> {
    let l = above?.trim();
    if !(l.ends_with('?') || l.ends_with('？')) {
        return None;
    }
    Some(kept(l, KEPT_LINE)).filter(|k| !k.is_empty())
}

#[cfg(test)]
mod tests;
