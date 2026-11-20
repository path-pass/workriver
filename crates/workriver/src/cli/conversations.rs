// The commands of conversations: asking why (and waiting for the answer), answering,
// replying to the user, and what a manager says or is told.
use super::*;

/// The commands of questions, answers, replies, and what is said to and by a manager: what each records (printed by `run` as before), or None when it
/// printed what it had to say itself.
pub(super) fn run(cmd: Cmd, cx: &Cx) -> anyhow::Result<Option<Value>> {
    let Cx { d, actor, .. } = *cx;
    Ok(Some(match cmd {
        Cmd::Manager(ManagerCmd::Tell { task, text }) => {
            let (unit, n) = task_ref(&task)?;
            d.append(actor, "manager.told", json!({ "unit": unit, "task": n, "text": text }))?
        }
        Cmd::Ask { task, question, file, lines, wait, timeout } => {
            let (unit, n) = task_ref(&task)?;
            let mut context = vec![];
            if let Some(path) = &file {
                context.push(match lines.as_deref().map(parse_lines).transpose()? {
                    Some((a, b)) => json!({ "kind": "lines", "label": format!("{path} L{a}-{b}"), "path": path, "from": a, "to": b }),
                    None => json!({ "kind": "file", "label": path, "path": path }),
                });
            }
            let mut ask = d.me(actor);
            ask["unit"] = json!(unit);
            ask["task"] = json!(n);
            ask["text"] = json!(question);
            ask["context"] = json!(context);
            let e = d.call("/api/ask", Some(ask))?;
            let id = e["body"]["id"].as_str().unwrap_or("?").to_string();
            if !wait {
                println!("{id}");
                return Ok(None);
            }
            println!("{}", wait_answer(d, &id, Duration::from_secs(timeout))?);
            return Ok(None);
        }
        Cmd::Answer { question, text, asked, conclusion } => d.append(actor, "question.answered", json!({ "question": question, "text": text, "asked": asked, "conclusion": conclusion }))?,
        Cmd::Reply { message, text, option, require_note } => {
            let choice = parse_choice(&option, &require_note)?;
            let out = d.append(actor, "message.replied", json!({ "message": message, "text": text, "choice": choice }))?;
            if choice.is_none() && lists_options(&[&text]) {
                println!("{}\n{OPTIONS_HINT}", made(&out));
                return Ok(None);
            }
            out
        }
        Cmd::Say { text, option, require_note } => {
            let choice = parse_choice(&option, &require_note)?;
            let out = d.append(actor, "message.said", json!({ "text": text, "choice": choice }))?;
            if choice.is_none() && lists_options(&[&text]) {
                println!("{}\n{OPTIONS_HINT}", made(&out));
                return Ok(None);
            }
            out
        }
        _ => unreachable!("not one of questions, answers, replies, and what is said to and by a manager"),
    }))
}

/// Polls until question `id` is answered (its text) or fails, or `timeout` passes.
pub(crate) fn wait_answer(d: &Daemon, id: &str, timeout: Duration) -> anyhow::Result<String> {
    let until = std::time::Instant::now() + timeout;
    loop {
        let state: core::State = serde_json::from_value(d.get("/api/state")?)?;
        let q = state.question(id).map_err(|e| anyhow!(e.0))?;
        if let Some(a) = &q.answer {
            return Ok(a.clone());
        }
        if let Some((code, why)) = &q.failed {
            bail!("question {id} got no answer ({}): {why}", name(code));
        }
        if std::time::Instant::now() >= until {
            bail!("no answer to {id} within {} s; it is still open, and the answer will come to your terminal", timeout.as_secs());
        }
        std::thread::sleep(Duration::from_millis(1000).min(until.saturating_duration_since(std::time::Instant::now()).max(Duration::from_millis(50))));
    }
}
