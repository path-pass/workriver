use super::*;

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/src/screens/__fixtures__/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn dialog(name: &str) -> Dialog {
    parse(&fixture(name)).unwrap_or_else(|| panic!("{name}: no dialog found"))
}

fn words(d: &Dialog) -> Vec<&str> {
    d.options.iter().map(|o| o.words.as_str()).collect()
}

fn keys(d: &Dialog) -> Vec<Key> {
    d.hints.iter().map(|h| h.key).collect()
}

fn line(text: &str, row: usize) -> &str {
    text.lines().nth(row).unwrap()
}

#[test]
fn claude_bash_numbered_with_its_command_in_the_region() {
    let text = fixture("claude-bash.txt");
    let d = dialog("claude-bash.txt");
    assert!(d.numbered);
    assert_eq!(words(&d), ["Yes", "Yes, and always allow access to /tmp/work/xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx/00000000-0000-0000-", "Yes, and switch to auto mode · auto mode handles these prompts for you", "No"]);
    assert_eq!(d.options.iter().map(|o| o.number).collect::<Vec<_>>(), [Some(1), Some(2), Some(3), Some(4)]);
    assert!(d.options[0].marked && !d.options[1].marked);
    assert_eq!(keys(&d), [Key::Escape, Key::Tab]);
    assert_eq!(d.hints[0].words, "Esc to cancel");
    assert_eq!(d.question.as_deref(), Some("Do you want to proceed?"));
    // From just under the solid rule (the command between the dashed rules is in it) to the hints.
    assert_eq!(line(&text, d.top).trim(), "Bash command");
    assert_eq!(line(&text, d.bottom).trim(), "Esc to cancel · Tab to amend");
    assert!(!d.cut);
    assert!(d.waiting());
}

#[test]
fn claude_write_and_its_long_option() {
    let d = dialog("claude-write.txt");
    assert_eq!(d.options.len(), 3);
    assert_eq!(d.options[2].words, "No");
    assert_eq!(d.question.as_deref(), Some("Do you want to create notes.md?"));
    assert_eq!(keys(&d), [Key::Escape, Key::Tab]);
}

#[test]
fn claude_question_passes_over_the_rule_before_its_last_option() {
    let d = dialog("claude-question.txt");
    assert_eq!(words(&d), ["Red", "Blue", "Type something.", "Chat about this"]);
    assert_eq!(d.question.as_deref(), Some("Which color do you prefer?"));
    // "↑/↓ to navigate" names two keys: text, not a button.
    assert_eq!(keys(&d), [Key::Enter, Key::Escape]);
    assert_eq!(d.hints[0].words, "Enter to select");
}

#[test]
fn claude_plan_keeps_the_plan_out_of_the_region() {
    let text = fixture("claude-plan.txt");
    let d = dialog("claude-plan.txt");
    assert_eq!(words(&d), ["Yes, and use auto mode", "Yes, manually approve edits", "Tell Claude what to change"]);
    assert!(d.options[0].marked);
    // The plan's own "1. Write hello.txt" is above the solid rule: never an option.
    assert!(line(&text, d.top).contains("Claude has written up a plan"));
    assert!(!d.cut);
    // Its last lines name no key: a dialog for a blocked agent, not for one the runtime calls idle.
    assert!(d.hints.is_empty());
    assert!(!d.waiting());
    assert_eq!(d.question.as_deref(), Some("Claude has written up a plan and is ready to execute. Would you like to proceed?"));
}

#[test]
fn claude_plan_with_the_selection_moved() {
    let d = dialog("claude-plan-moved.txt");
    assert_eq!(d.options.iter().map(|o| o.marked).collect::<Vec<_>>(), [false, false, true]);
}

#[test]
fn claude_trust_is_a_marked_list() {
    let text = fixture("claude-trust.txt");
    let d = dialog("claude-trust.txt");
    assert!(!d.numbered);
    assert_eq!(words(&d), ["No, exit", "Yes, I trust this folder"]);
    assert_eq!(d.options.iter().map(|o| (o.number, o.marked)).collect::<Vec<_>>(), [(None, false), (None, true)]);
    assert_eq!(keys(&d), [Key::Enter, Key::Escape]);
    assert_eq!(line(&text, d.top).trim(), "Accessing workspace:");
    assert_eq!(d.question, None);
    assert!(d.waiting());
}

#[test]
fn codex_dialogs() {
    for (name, first, question) in [
        ("codex-command.txt", "Yes, proceed (y)", None),
        ("codex-edit.txt", "Yes, proceed (y)", None),
        ("codex-hooks-review.txt", "Review hooks", None),
        ("codex-plan.txt", "Yes, implement this plan          Switch to Default and start coding", Some("Implement this plan?")),
        ("codex-question.txt", "Red (Recommended)  选择红色。", Some("你更喜欢哪种颜色？")),
    ] {
        let d = dialog(name);
        assert!(d.numbered, "{name}");
        assert_eq!(d.options.len(), 3, "{name}");
        assert_eq!(d.options[0].words, first, "{name}");
        assert!(d.options[0].marked, "{name}");
        assert_eq!(d.question.as_deref(), question, "{name}");
        assert!(d.waiting(), "{name}: its hint names a key");
    }
    assert_eq!(keys(&dialog("codex-command.txt")), [Key::Enter, Key::Escape]);
    assert_eq!(dialog("codex-command.txt").hints[0].words, "Press enter to confirm");
    assert_eq!(keys(&dialog("codex-hooks-review.txt")), [Key::Enter, Key::Escape]);
    assert_eq!(keys(&dialog("codex-question.txt")), [Key::Tab, Key::Enter, Key::Escape]);
}

#[test]
fn a_dialog_quoted_above_an_input_box_is_none() {
    assert_eq!(parse(&fixture("claude-idle-quoted.txt")), None);
    assert_eq!(parse(&fixture("codex-idle-quoted.txt")), None);
    assert_eq!(parse(""), None);
    assert_eq!(parse("$ ls\na.txt b.txt\n$ \n"), None);
}

#[test]
fn a_region_the_twelve_lines_cut_says_so() {
    let text = fixture("claude-bash-long.txt");
    let d = dialog("claude-bash-long.txt");
    assert!(d.cut);
    assert_eq!(d.options.len(), 2);
    // Twelve lines above the first option, no more.
    let first = d.options[0].row;
    assert_eq!(first - d.top, 12);
    assert!(!line(&text, d.top).contains("Bash command"));
}

#[test]
fn rows_and_the_marker() {
    let text = fixture("claude-trust.txt");
    let d = dialog("claude-trust.txt");
    assert_eq!(line(&text, d.options[1].row).trim(), "❯ Yes, I trust this folder");
    assert_eq!(marked_row(&text), Some(d.options[1].row));
    // Blanking the marker: the same screen whichever line it is on.
    let moved = text.replace(" ❯ Yes, I trust", "   Yes, I trust").replace("   No, exit", " ❯ No, exit");
    assert_ne!(moved, text);
    assert_eq!(unmarked(&moved), unmarked(&text));
    assert_eq!(parse(&moved).unwrap().options[0].marked, true);
    // Any other change is not hidden by it.
    assert_ne!(unmarked(&moved.replace("No, exit", "No, quit")), unmarked(&text));
}

#[test]
fn keys_by_name() {
    for (name, key) in [("up", Key::Up), ("down", Key::Down), ("enter", Key::Enter), ("escape", Key::Escape), ("tab", Key::Tab), ("1", Key::Digit(1)), ("9", Key::Digit(9))] {
        assert_eq!(Key::named(name), Some(key));
        assert_eq!(key.name(), name);
        assert_eq!(serde_json::to_value(key).unwrap(), name);
        assert_eq!(serde_json::from_value::<Key>(serde_json::json!(name)).unwrap(), key);
    }
    for bad in ["0", "10", "a", "y", "ctrl+c", "Enter ", "", "space", "backspace"] {
        assert_eq!(Key::named(bad), None, "{bad}");
        assert!(serde_json::from_value::<Key>(serde_json::json!(bad)).is_err(), "{bad}");
    }
}

#[test]
fn marked_turns_every_default_ignorable_code_point_visible() {
    // Every code point Unicode 17 lists as Default_Ignorable_Code_Point (Bidi_Control is a
    // subset of it), as the fixture copied from DerivedCoreProperties.txt has them.
    let table = fixture("default-ignorable.txt");
    let mut all = vec![];
    for l in table.lines().filter(|l| !l.starts_with('#')) {
        let range = l.split(';').next().unwrap().trim();
        let (a, b) = range.split_once("..").unwrap_or((range, range));
        let (a, b) = (u32::from_str_radix(a, 16).unwrap(), u32::from_str_radix(b, 16).unwrap());
        all.extend(a..=b);
    }
    assert!(all.len() > 4000);
    for cp in &all {
        let Some(c) = char::from_u32(*cp) else { continue };
        assert_eq!(marked(&format!("a{c}b")), format!("a⟨U+{cp:04X}⟩b"), "U+{cp:04X}");
    }
    // Their neighbours, and what a terminal draws, stay as they are.
    for c in ['\u{00AC}', '\u{00AE}', '\u{034E}', '\u{0350}', '\u{200A}', '\u{2010}', '\u{2070}', 'é', '中', '❯', '─', 'א', '\u{E1000}'] {
        assert_eq!(marked(&format!("a{c}b")), format!("a{c}b"), "U+{:04X}", c as u32);
    }
    // Order kept, nothing dropped.
    assert_eq!(marked("rm \u{202E}gpj.exe\u{202C} x"), "rm ⟨U+202E⟩gpj.exe⟨U+202C⟩ x");
}

#[test]
fn what_is_kept_of_a_line() {
    assert_eq!(kept("Yes, and don't ask again for `curl -H …` commands in /path", 40), "Yes, and don't ask again for");
    assert_eq!(kept("Do you want to proceed?", 120), "Do you want to proceed?");
    for (line, want) in [
        ("Allow TOKEN=abc123?", "Allow TOKEN"),
        ("Run \"secret\"?", "Run"),
        ("Run 'secret'?", "Run"),
        ("Use $HOME?", "Use"),
        ("Open ~/x?", "Open"),
        ("Open C:\\x?", "Open C:"),
        ("Open /etc/passwd?", "Open"),
        ("it's fine?", "it's fine?"),
    ] {
        assert_eq!(kept(line, 120), want, "{line}");
    }
    // Capped, one line, control characters as spaces, marked.
    assert_eq!(kept(&"a".repeat(200), 40), "a".repeat(40));
    assert_eq!(kept("one\ttwo\nthree", 120), "one two three");
    assert_eq!(kept("x\u{202E}y", 120), "x⟨U+202E⟩y");
}

#[test]
fn a_question_line_is_kept_only_when_it_asks() {
    assert_eq!(question_line(Some("Do you want to proceed?")), Some("Do you want to proceed?".into()));
    assert_eq!(question_line(Some("你更喜欢哪种颜色？")), Some("你更喜欢哪种颜色？".into()));
    assert_eq!(question_line(Some("$ touch b.txt")), None);
    // A question, cut where its command begins.
    assert_eq!(question_line(Some("Run `rm -rf /`?")), Some("Run".into()));
    assert_eq!(question_line(Some("`rm -rf /`?")), None);
    assert_eq!(question_line(None), None);
}
