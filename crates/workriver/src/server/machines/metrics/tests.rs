use super::*;

/// What the probe printed on a Mac (2026-11-17, macOS 26), shortened to the lines it reads
/// and a few it skips.
const DARWIN: &str = "os darwin
hw.memsize: 34359738368
kern.memorystatus_vm_pressure_level: 2
kern.memorystatus_level: 50
vm.swapusage: total = 4096.00M  used = 3659.88M  free = 436.12M  (encrypted)
vm.compressor.segment.total: 279400
vm.compressor.segment.limit: 1647102
vm.compressor_segment_pages_compressed: 1980784
vm.compressor_segment_pages_compressed_limit: 13176816
Mach Virtual Memory Statistics: (page size of 16384 bytes)
Pages free:                                    33895.
Pages active:                                 502965.
Pages inactive:                               482372.
Pages speculative:                             18959.
Pages throttled:                                   0.
Pages wired down:                             199417.
Pages purgeable:                                2908.
\"Translation faults\":                     1627581914.
Pages stored in compressor:                  1980724.
Pages occupied by compressor:                 819908.
Swapouts:                                     241400.
iostat  10 12 78  5.17 4.79 4.41
Filesystem     1024-blocks      Used Available Capacity  Mounted on
/dev/disk3s5     482797652 375308792  81939804    83%    /System/Volumes/Data
/dev/disk3s1s1   482797652  12349052  81939804    14%    /
";

/// A process list that was not read, or does not hold the process asked about, is an
/// error: never "it started nothing".
#[test]
fn children_are_told_only_from_a_list_that_was_read() {
    let null = std::process::Stdio::null;
    let mut sh = std::process::Command::new("sh").args(["-c", "sleep 30 & wait"]).stdin(null()).stdout(null()).stderr(null()).spawn().unwrap();
    let mut alone = std::process::Command::new("sleep").arg("30").stdin(null()).stdout(null()).stderr(null()).spawn().unwrap();
    // The shell forks its job a moment after it starts.
    assert!((0..50).any(|_| has_children(&scan::Runner::Local, sh.id()).unwrap() || { std::thread::sleep(Duration::from_millis(100)); false }));
    assert!(!has_children(&scan::Runner::Local, alone.id()).unwrap());
    assert!(has_children(&scan::Runner::Local, 4_000_000_000).is_err(), "not in the list");
    let failing = scan::Runner::Ssh { program: "false".into(), options: vec![], target: "box".into() };
    assert!(has_children(&failing, alone.id()).is_err(), "the list was not read");
    for c in [&mut sh, &mut alone] {
        c.kill().unwrap();
        c.wait().unwrap();
    }
}

/// What `PS` printed on the same Mac, a few of its lines (one agent's shell made the
/// parent of its agent, as under the agent runtime).
const PS_DARWIN: &str = "    1     0  22960 /sbin/launchd
 6286     1   1200 /usr/bin/login
 6287  6286   1328 -zsh
 6567  6287 783504 claude
57934  1354 2396528 /Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Versions/157.0.8103.51/Helpers/Google Chrome Helper (Renderer).app/Contents/MacOS/Google Chrome Helper (Renderer)
 1354     1 484432 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome
 1431     1 382016 /System/Library/CoreServices/Spotlight.app/Contents/MacOS/Spotlight
  522     1  27728 /usr/libexec/logd
";

/// What `PS` printed in `docker run --rm debian:stable-slim` (no `ps` there), a
/// background `sleep` running; and a line of a thread whose name has a space.
const PS_LINUX: &str = "1 0 1068 sh
14 1 1236 sh
15 14 1020 cat
7 1 1116 sleep
31 1 2048 tmux: server
";

/// What the probe printed in `docker run --rm --memory=64m --memory-swap=1g
/// debian:stable-slim` (kernel 6.10) while a process there ran out of memory, OOM kills
/// earlier in the same VM.
const LINUX_PSI: &str = "os linux
cpu  830 0 1086 60601 1137 0 105 0 0 0
MemTotal:        8025424 kB
MemAvailable:    6863792 kB
SwapTotal:       1048572 kB
SwapFree:         732392 kB
oom_kill 4
some avg10=0.59 avg60=0.12 avg300=0.02 total=123928
full avg10=0.59 avg60=0.12 avg300=0.02 total=122836
Filesystem     1024-blocks     Used Available Capacity Mounted on
overlay         1055761844 13925952 988132468       2% /
overlay         1055761844 13925952 988132468       2% /
";

/// The same container with the probe pointed at paths that do not exist instead of
/// /proc/vmstat and /proc/pressure/memory, as a kernel or container without them answers.
const LINUX_NO_PSI: &str = "os linux
cpu  498 0 558 48139 768 0 58 0 0 0
MemTotal:        8025424 kB
MemAvailable:    7373248 kB
SwapTotal:       1048572 kB
SwapFree:        1048572 kB
Filesystem     1024-blocks     Used Available Capacity Mounted on
overlay         1055761844 13925964 988132456       2% /
overlay         1055761844 13925964 988132456       2% /
";

/// Counters made up to check the arithmetic, as an old kernel prints them: no swap lines
/// read, no PSI, no oom_kill, no df.
const LINUX_1: &str = "os linux
cpu  1000 0 500 8000 500 0 0 0 0 0
MemTotal:       16303428 kB
MemAvailable:    4075857 kB
";

const LINUX_2: &str = "os linux
cpu  1600 0 800 8500 600 0 0 0 0 0
MemTotal:       16303428 kB
MemAvailable:    4075857 kB
";

#[test]
fn reads_a_mac() {
    let r = parse(DARWIN).unwrap();
    assert_eq!(r.mem_total, 34_359_738_368);
    assert_eq!(r.mem_used, 34_359_738_368 - (33895 + 482372 + 18959 + 2908) * 16384);
    assert_eq!(r.cpu, Cpu::Percent(22.0));
    assert_eq!(cpu_share(None, &r), Some(22.0), "a Mac measures its own second");
    let d = &r.detail;
    assert_eq!(d.mem_compressed, Some(819_908 * 16384));
    assert_eq!(d.pressure, Some(Pressure::Warning));
    assert_eq!(d.system_free, Some(50.0));
    // The segments are nearer their limit (17.0 %) than the compressed pages (15.0 %).
    assert_eq!(d.compressor, Some(279_400.0 * 100.0 / 1_647_102.0));
    assert_eq!((d.swap_used, d.swap_total), (Some((3659.88 * 1_048_576.0) as u64), Some(4096 << 20)));
    assert_eq!((d.pressure_some, d.oom_kills), (None, None), "Linux only");
    // `/` is a volume of the same APFS container as the home folder's: listed once.
    assert_eq!(d.disks, [Disk { mount: "/System/Volumes/Data".into(), free: 81_939_804 * 1024, total: 482_797_652 * 1024 }]);
    // Without the pressure, swap and compressor sysctls (an older macOS, a sandbox): none, still read.
    let bare: String = DARWIN.lines().filter(|l| !l.starts_with("kern.") && !l.starts_with("vm.")).map(|l| format!("{l}\n")).collect();
    let d = parse(&bare).unwrap().detail;
    assert_eq!((d.pressure, d.swap_used, d.swap_total, d.compressor, d.system_free), (None, None, None, None, None));
    assert_eq!(parse(&DARWIN.replace("level: 2", "level: 4")).unwrap().detail.pressure, Some(Pressure::Critical));
    // Fewer segments than compressed pages: the pages decide.
    assert_eq!(parse(&DARWIN.replace("total: 279400", "total: 1")).unwrap().detail.compressor, Some(1_980_784.0 * 100.0 / 13_176_816.0));
}

#[test]
fn reads_linux_pressure_oom_kills_swap_and_disk() {
    let r = parse(LINUX_PSI).unwrap();
    assert_eq!(r.mem_used, (8_025_424 - 6_863_792) * 1024, "used is total − MemAvailable");
    let d = &r.detail;
    assert_eq!((d.pressure_some, d.pressure_full, d.oom_kills), (Some(0.59), Some(0.59), Some(4)));
    assert_eq!((d.swap_used, d.swap_total), (Some((1_048_572 - 732_392) * 1024), Some(1_048_572 * 1024)));
    assert_eq!(d.disks, [Disk { mount: "/".into(), free: 988_132_468 * 1024, total: 1_055_761_844 * 1024 }], "home is on /: once");
    assert_eq!((d.mem_compressed, d.pressure), (None, None), "macOS only");
    let d = parse(LINUX_NO_PSI).unwrap().detail;
    assert_eq!((d.pressure_some, d.pressure_full, d.oom_kills), (None, None, None), "unknown, not an error");
    assert_eq!((d.swap_used, d.disks.len()), (Some(0), 1));
    let d = parse(LINUX_1).unwrap().detail;
    assert_eq!(d, Detail { os: Some(Os::Linux), ..Detail::default() }, "nothing beyond CPU and memory: still a reading");
}

#[test]
fn df_lines_with_spaces_and_another_filesystem_for_home() {
    let text = "Filesystem 1024-blocks Used Available Capacity Mounted on
map auto_home         0        0         0   100%    /System/Volumes/Data/home
/dev/sda1    100 60 40 60% /
not a df line
";
    let d = disks(text);
    assert_eq!(d.iter().map(|d| (d.mount.as_str(), d.free)).collect::<Vec<_>>(), [("/System/Volumes/Data/home", 0), ("/", 40 * 1024)]);
}

#[test]
fn reads_linux_counters_between_two_samples() {
    let first = parse(LINUX_1).unwrap();
    assert_eq!(first.mem_total, 16_303_428 * 1024);
    assert_eq!(first.mem_used, (16_303_428 - 4_075_857) * 1024);
    assert_eq!(cpu_share(None, &first), None, "the first sample of counters has no CPU yet");
    let second = parse(LINUX_2).unwrap();
    // busy went from 1500 to 2400 (+900), total from 10000 to 11500 (+1500): 60 %.
    assert_eq!(cpu_share(Some(&first), &second), Some(60.0));
}

#[test]
fn the_probe_reads_the_machine_the_tests_run_on() {
    let out = std::process::Command::new("sh").args(["-c", PROBE]).output().unwrap();
    let r = parse(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert!(r.mem_total > 0 && r.mem_used <= r.mem_total, "{r:?}");
    assert!(r.detail.disks.first().is_some_and(|d| d.total > 0), "the home folder's filesystem: {r:?}");
    if cfg!(target_os = "macos") {
        let d = &r.detail;
        assert!(d.pressure.is_some() && d.swap_total.is_some() && d.mem_compressed.is_some() && d.system_free.is_some(), "{r:?}");
        assert!(d.compressor.is_some_and(|c| c > 0.0 && c < 100.0), "{r:?}");
    }
}

#[test]
fn the_probe_is_one_line_a_csh_login_shell_takes() {
    assert!(!PROBE.contains('\n'), "a csh-family shell refuses a quoted word with a newline");
    // Over ssh the command reaches the login shell as one string of quoted words: a
    // stand-in hands its last argument to tcsh, as such a login shell would run it.
    if !std::path::Path::new("/bin/tcsh").exists() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let program = dir.path().join("ssh");
    std::fs::write(&program, "#!/bin/sh\nfor a; do last=$a; done\nexec /bin/tcsh -c \"$last\"\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let runner = scan::Runner::Ssh { program: program.to_string_lossy().into(), options: vec![], target: "csh-box".into() };
    let out = runner.command(&["sh", "-c", PROBE]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let r = parse(&String::from_utf8_lossy(&out.stdout)).unwrap();
    assert!(r.mem_total > 0, "{r:?}");
}

#[test]
fn says_what_it_cannot_read() {
    assert!(parse("os plan9\n").unwrap_err().contains("plan9"));
    assert!(parse("os linux\nMemTotal: 1 kB\n").is_err());
    assert!(parse("os darwin\nmemsize 1\n").is_err());
}

const STEP_MS: i64 = 5_000;
const CPU: Limit = Limit { on: true, percent: 90, minutes: 1 };

/// CPU or memory judged by `limit`, as `judge` does: raised at the one level they have.
fn eval(samples: &[Sample], metric: Metric, limit: &Limit, raised: Option<i64>, reachable: bool) -> Option<i64> {
    let t = Thresholds { cpu: *limit, memory: *limit, ..Thresholds::default() };
    if !t.on(metric) {
        return None;
    }
    let r = evaluate(samples, &|s| s.reading(metric, &t).map(|r| r.1), t.minutes(metric), raised.map(|r| (r, Level::Warn)), reachable, STEP_MS);
    r.map(|r| r.0)
}

/// A sample of CPU and memory only.
fn bare(at: i64, cpu: Option<f32>, mem_used: u64, mem_total: u64) -> Sample {
    Sample { at, cpu, mem_used, mem_total, oom_kills_watched: None, detail: Detail::default() }
}

/// Samples every 5 s, CPU as given, from t = 0.
fn run(cpu: &[f32]) -> Vec<Sample> {
    cpu.iter().enumerate().map(|(k, &c)| bare(k as i64 * STEP_MS, Some(c), 1, 2)).collect()
}

#[test]
fn raises_only_after_the_whole_window() {
    // 11 samples over 90 % cover 55 s of a 60 s window: not yet; the 12th makes a minute.
    assert_eq!(eval(&run(&[95.0; 11]), Metric::Cpu, &CPU, None, true), None);
    assert_eq!(eval(&run(&[95.0; 12]), Metric::Cpu, &CPU, None, true), Some(0));
    // The run starts after the last sample below.
    let mut s = vec![10.0, 10.0];
    s.extend([95.0; 12]);
    assert_eq!(eval(&run(&s), Metric::Cpu, &CPU, None, true), Some(2 * STEP_MS));
}

#[test]
fn a_spike_does_not_raise() {
    let mut s = vec![95.0; 6];
    s.push(50.0);
    s.extend([95.0; 6]);
    assert_eq!(eval(&run(&s), Metric::Cpu, &CPU, None, true), None);
}

#[test]
fn clears_after_a_minute_below() {
    let mut s = vec![95.0; 12];
    s.extend([50.0; 11]);
    assert_eq!(eval(&run(&s), Metric::Cpu, &CPU, Some(0), true), Some(0), "55 s below is not yet a minute");
    s.push(50.0);
    assert_eq!(eval(&run(&s), Metric::Cpu, &CPU, Some(0), true), None);
    // One sample back over the threshold keeps it raised.
    let mut s = vec![50.0; 11];
    s.push(95.0);
    assert_eq!(eval(&run(&s), Metric::Cpu, &CPU, Some(0), true), Some(0));
}

#[test]
fn frozen_while_unreachable_and_never_when_switched_off() {
    let hot = run(&[95.0; 12]);
    assert_eq!(eval(&hot, Metric::Cpu, &CPU, None, false), None, "not raised while away");
    assert_eq!(eval(&run(&[10.0; 20]), Metric::Cpu, &CPU, Some(0), false), Some(0), "nor cleared");
    let off = Limit { on: false, ..CPU };
    assert_eq!(eval(&hot, Metric::Cpu, &off, Some(0), true), None);
}

#[test]
fn a_gap_starts_the_window_again() {
    // Six samples, the machine away for a minute, six more: no run of a minute.
    let mut s = run(&[95.0; 6]);
    s.extend(run(&[95.0; 6]).into_iter().map(|x| Sample { at: x.at + 90_000, ..x }));
    assert_eq!(eval(&s, Metric::Cpu, &CPU, None, true), None);
}

#[test]
fn memory_is_used_over_total() {
    let full: Vec<Sample> = (0..12).map(|k| bare(k * STEP_MS, None, 95, 100)).collect();
    assert_eq!(eval(&full, Metric::Memory, &CPU, None, true), Some(0));
    assert_eq!(eval(&full, Metric::Cpu, &CPU, None, true), None, "no CPU value yet: nothing to judge");
}

#[test]
fn thresholds_are_checked_and_kept_in_the_settings() {
    assert!(Thresholds::default().check().is_ok());
    let mut t = Thresholds::default();
    t.cpu.percent = 0;
    assert!(t.check().unwrap_err().contains("CPU"));
    t.cpu.percent = 80;
    t.memory.minutes = 11;
    assert!(t.check().unwrap_err().contains("memory"));
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(crate::runtime::Settings::thresholds(dir.path()), Thresholds::default(), "the defaults until set");
    let set = Thresholds { cpu: Limit { on: false, percent: 80, minutes: 2 }, memory: Limit { on: true, percent: 95, minutes: 10 }, ..Thresholds::default() };
    crate::runtime::Settings::set_thresholds(dir.path(), set).unwrap();
    assert_eq!(crate::runtime::Settings::thresholds(dir.path()), set);
}

/// A sample of a Mac at `at` with the given CPU, pressure and OOM count.
fn mac_at(at: i64, cpu: f32, pressure: Pressure, oom: u64) -> Sample {
    let detail = Detail { os: Some(Os::Darwin), pressure: Some(pressure), oom_kills: Some(oom), swap_used: Some(100), disks: vec![Disk { mount: "/".into(), free: at as u64, total: 9 }], ..Detail::default() };
    Sample { at, cpu: Some(cpu), mem_used: 40, mem_total: 100, oom_kills_watched: Some(oom), detail }
}

#[test]
fn a_minute_becomes_one_line_of_means() {
    let mut minute = vec![];
    let t0 = 1_800_000_000_000; // a whole minute
    assert_eq!(next_minute(&mut minute, &mac_at(t0 + 1_000, 10.0, Pressure::Normal, 1)), None);
    assert_eq!(next_minute(&mut minute, &mac_at(t0 + 30_000, 30.0, Pressure::Critical, 2)), None);
    assert_eq!(next_minute(&mut minute, &mac_at(t0 + 59_999, 50.0, Pressure::Warning, 3)), None);
    let line = next_minute(&mut minute, &mac_at(t0 + 60_000, 99.0, Pressure::Normal, 4)).expect("the next minute began");
    assert_eq!((line.at, line.cpu, line.mem_used), (t0, Some(30.0), 40));
    assert_eq!(line.detail.pressure, Some(Pressure::Critical), "the worst of the minute");
    assert_eq!((line.detail.oom_kills, line.oom_kills_watched, line.detail.swap_used), (Some(3), Some(3), Some(100)), "counters at the minute's end");
    assert_eq!(line.detail.disks[0].free, (t0 + 59_999) as u64, "the disks at the minute's end");
    assert_eq!(minute.len(), 1, "the new minute has its first sample");
}

#[test]
fn the_history_is_kept_a_week_and_read_back_after_a_restart() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let now = now_ms();
    let old = mac_at(now - HISTORY_MS - 60_000, 5.0, Pressure::Normal, 0);
    let day_ago = mac_at(now - 24 * 3_600_000 + 60_000, 20.0, Pressure::Warning, 0);
    let recent = mac_at(now - 60_000, 40.0, Pressure::Normal, 1);
    keep_minute(dir.path(), "box", &old, None).unwrap();
    keep_minute(dir.path(), "box", &day_ago, None).unwrap();
    let path = dir.path().join("metrics/box.jsonl");
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(std::fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 2);
    // The trim: the week-old line goes, the file is rewritten 0600, the new line follows.
    keep_minute(dir.path(), "box", &recent, Some(now - HISTORY_MS)).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().map(|l| parse_line(l).unwrap().at).collect::<Vec<_>>(), [day_ago.at, recent.at], "{text}");
    assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    // A line cut off by a crash is skipped, not an error.
    std::fs::OpenOptions::new().append(true).open(&path).and_then(|mut f| std::io::Write::write_all(&mut f, b"{\"at\":1")).unwrap();
    // Another service on the same folder (a restart): the history is there to read; the
    // samples alerts judge start empty, as before.
    let (tx, _) = broadcast::channel(4);
    let m = Metrics::new(dir.path().to_path_buf(), printing(dir.path(), "here", DARWIN), None, Arc::new(workriver_core::runtime::NoRuntime), tx, Duration::from_secs(5));
    assert_eq!(m.history("box", 168).unwrap(), [day_ago.clone(), recent.clone()]);
    assert_eq!(m.history("box", 1).unwrap(), [recent], "the last hour only");
    assert!(m.history("box", 0).is_err() && m.history("box", 169).unwrap_err().contains("168"));
    assert_eq!(m.history("never-seen", 24).unwrap(), []);
    assert!(m.report().machines[0].samples.is_empty());
    let summary = crate::cli::history_text(&serde_json::json!({ "label": "box" }), &serde_json::to_value(m.history("box", 168).unwrap()).unwrap());
    assert_eq!(summary, "box  cpu mean 30% max 40%  memory mean 40% max 40%  (2 min)");
    // A label is a file name, never a path.
    assert_eq!(history_path(dir.path(), "../x"), dir.path().join("metrics/.._x.jsonl"));
}

/// An ssh stand-in that prints `text` whatever it is asked.
fn printing(dir: &std::path::Path, name: &str, text: &str) -> scan::Runner {
    let data = dir.join(format!("{name}.txt"));
    std::fs::write(&data, text).unwrap();
    let program = dir.join(name);
    std::fs::write(&program, format!("#!/bin/sh\ncat '{}'\n", data.display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    scan::Runner::Ssh { program: program.to_string_lossy().into(), options: vec![], target: name.into() }
}

#[test]
fn samples_this_machine_and_followed_ones_through_their_runner() {
    let dir = tempfile::tempdir().unwrap();
    let local: Arc<dyn workriver_core::runtime::Runtime> = Arc::new(crate::server::tests::support::Fake::default());
    let all = Machines::with(local.clone(), vec![("box", local.clone()), ("gone", local)]);
    all.set_runner("box", printing(dir.path(), "box", LINUX_1));
    all.drop_tunnel("gone");
    let (tx, mut rx) = broadcast::channel(4);
    let m = Metrics::new(dir.path().to_path_buf(), printing(dir.path(), "here", DARWIN), Some(all.clone()), all.clone(), tx, Duration::from_millis(10));
    for label in [HERE, "box", "gone"] {
        m.sample(label);
    }
    all.set_runner("box", printing(dir.path(), "box2", LINUX_2));
    m.sample("box");
    let r = m.report();
    let get = |l: &str| r.machines.iter().find(|x| x.label == l).unwrap().clone();
    assert_eq!(r.machines.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(), [HERE, "box", "gone"]);
    let here = get(HERE);
    assert!(here.here && here.reachable && here.samples.len() == 1 && here.samples[0].cpu == Some(22.0));
    let b = get("box");
    assert_eq!(b.samples.iter().map(|s| s.cpu).collect::<Vec<_>>(), [None, Some(60.0)]);
    assert_eq!(b.samples[0].oom_kills_watched, None, "no oom_kill line, no count");
    // OOM kills: counted from the first reading, through a restart of the machine.
    for (k, kills) in [4, 6, 1].into_iter().enumerate() {
        all.set_runner("box", printing(dir.path(), &format!("oom{k}"), &LINUX_PSI.replace("oom_kill 4", &format!("oom_kill {kills}"))));
        m.sample("box");
    }
    let oom: Vec<_> = m.report().machines[1].samples.iter().rev().take(3).map(|s| (s.detail.oom_kills, s.oom_kills_watched)).collect();
    assert_eq!(oom, [(Some(1), Some(3)), (Some(6), Some(2)), (Some(4), Some(0))]);
    assert!(rx.try_recv().is_ok(), "the kills raised the OOM alert");
    let text = crate::cli::load_text(&serde_json::to_value(m.report()).unwrap());
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[1], "  available 8.2 GB  pressure warning  compressed 12.5 GB  compressor 17% of its limit  system free 50%  swap 3.6/4.0 GB  disk 78.1 GB free of 460.4 GB", "{text}");
    assert!(lines[2].contains("  ALERT: oom CRIT since 20"), "{text}");
    assert_eq!(lines[3], "  available 6.5 GB  pressure some 0.59% full 0.59% (last 10 s)  oom kills 1 (3 while watched)  swap 0.3/1.0 GB  disk 942.4 GB free of 1006.9 GB", "{text}");
    let g = get("gone");
    assert!(!g.reachable && g.samples.is_empty() && g.down_since.is_some(), "{g:?}");
    // After a failed sample, the next one has no CPU: the counters of before the outage
    // would make it an average over the whole outage.
    all.set_runner("box", printing(dir.path(), "broken", "os linux\n"));
    m.sample("box");
    all.set_runner("box", printing(dir.path(), "box3", LINUX_2.replace("1600 0 800 8500", "2000 0 900 9000").as_str()));
    m.sample("box");
    assert_eq!(m.report().machines[1].samples.last().unwrap().cpu, None, "no CPU right after an outage");
    // A machine in alert that is no longer followed leaves Needs you at once, before its
    // loop next wakes and drops its track.
    m.tracks.lock().unwrap().get_mut("box").unwrap().raised.insert(Metric::Cpu, Raised { since: now_ms(), level: Level::Warn, top: Some(vec![]) });
    assert_eq!(m.alerts().iter().map(|a| (a.machine.as_str(), a.metric)).collect::<Vec<_>>(), [("box", Metric::Cpu), ("box", Metric::Oom)], "the OOM alert is held while the machine is away");
    all.stop("box");
    assert!(m.alerts().is_empty(), "the alert of a machine no longer followed");
    assert!(rx.try_recv().is_err(), "nothing raised, nothing said");
    // Thresholds low enough for this machine's memory (77 %): raised at once? Not before
    // the window is covered: one sample covers 5 s of a minute.
    m.set_thresholds(Thresholds { cpu: Limit { on: false, percent: 90, minutes: 1 }, memory: Limit { on: true, percent: 50, minutes: 1 }, ..Thresholds::default() }).unwrap();
    assert!(m.alerts().is_empty());
    assert!(m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 0, minutes: 1 }, ..Thresholds::default() }).is_err());
}

#[test]
fn says_when_an_alert_is_raised_and_cleared_only() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, mut rx) = broadcast::channel(4);
    let m = Metrics::new(dir.path().to_path_buf(), printing(dir.path(), "here", DARWIN), None, Arc::new(workriver_core::runtime::NoRuntime), tx, Duration::from_secs(5));
    // A minute of samples at 77 % memory, as the sampler would have kept them.
    {
        let mut tracks = m.tracks.lock().unwrap();
        let t = tracks.entry(HERE.into()).or_default();
        t.reachable = true;
        let now = now_ms();
        t.samples.extend((0..12).map(|k| bare(now - (11 - k) * 5_000, Some(10.0), 77, 100)));
    }
    m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 90, minutes: 1 }, memory: Limit { on: true, percent: 70, minutes: 1 }, ..Thresholds::default() }).unwrap();
    assert!(rx.try_recv().is_ok(), "raised: the page hears of it");
    let a = m.alerts();
    assert_eq!((a.len(), a[0].metric, a[0].level, a[0].threshold, a[0].minutes, a[0].here), (1, Metric::Memory, Level::Warn, Some(70.0), Some(1), true));
    assert_eq!(m.report().machines[0].alerts.iter().map(|(k, r)| (k.as_str(), r.level)).collect::<Vec<_>>(), [("memory", Level::Warn)]);
    // The same again changes nothing and says nothing.
    m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 90, minutes: 1 }, memory: Limit { on: true, percent: 70, minutes: 1 }, ..Thresholds::default() }).unwrap();
    assert!(rx.try_recv().is_err());
    // Switched off: cleared, and said.
    m.set_thresholds(Thresholds { cpu: Limit { on: true, percent: 90, minutes: 1 }, memory: Limit { on: false, percent: 70, minutes: 1 }, ..Thresholds::default() }).unwrap();
    assert!(rx.try_recv().is_ok() && m.alerts().is_empty());
}
/// A sample at `at` (ms) with `detail`, memory half used.
fn with(at: i64, detail: Detail) -> Sample {
    Sample { at, cpu: Some(1.0), mem_used: 1, mem_total: 2, oom_kills_watched: None, detail }
}

#[test]
fn each_threshold_has_its_warn_and_crit_edges() {
    use Level::{Crit, Warn};
    let t = Thresholds::default();
    let level = |s: Sample, m: Metric| s.reading(m, &t).map(|r| r.1);
    let os = |os: Os, f: &dyn Fn(&mut Detail)| {
        let mut d = Detail { os: Some(os), ..Detail::default() };
        f(&mut d);
        with(0, d)
    };
    let gb = |g: f64| (g * GIB as f64).round() as u64;
    for (v, want) in [(60.0, None), (60.1, Some(Warn)), (80.0, Some(Warn)), (80.1, Some(Crit))] {
        assert_eq!(level(os(Os::Darwin, &|d| d.compressor = Some(v)), Metric::Compressor), Some(want), "compressor {v}");
    }
    for (v, want) in [(4.0, None), (4.01, Some(Warn)), (10.0, Some(Warn)), (10.01, Some(Crit))] {
        assert_eq!(level(os(Os::Darwin, &|d| d.swap_used = Some(gb(v))), Metric::Swap), Some(want), "swap {v} GB");
    }
    for (p, want) in [(Pressure::Normal, None), (Pressure::Warning, Some(Warn)), (Pressure::Critical, Some(Crit))] {
        assert_eq!(level(os(Os::Darwin, &|d| d.pressure = Some(p)), Metric::Pressure), Some(want), "{p:?}");
    }
    for (v, want) in [(25.0, None), (24.9, Some(Warn)), (20.0, Some(Warn)), (19.9, Some(Crit))] {
        assert_eq!(level(os(Os::Darwin, &|d| d.system_free = Some(v)), Metric::SystemFree), Some(want), "system free {v}");
    }
    // Linux available: MemAvailable over MemTotal.
    for (available, want) in [(2500, None), (2499, Some(Warn)), (1200, Some(Warn)), (1199, Some(Crit))] {
        let s = Sample { mem_used: 10_000 - available, mem_total: 10_000, ..os(Os::Linux, &|_| ()) };
        assert_eq!(level(s, Metric::Available), Some(want), "available {available} of 10000");
    }
    assert_eq!(level(Sample { mem_used: 9_999, mem_total: 10_000, ..os(Os::Darwin, &|_| ()) }, Metric::Available), None, "Linux only");
    for (v, want) in [(5.0, None), (5.1, Some(Warn)), (20.0, Some(Warn)), (20.1, Some(Crit))] {
        assert_eq!(level(os(Os::Linux, &|d| d.pressure_full = Some(v)), Metric::Psi), Some(want), "PSI full {v}");
    }
    // Disk: used past 80 / 90 %, or free under 50 / 20 GB.
    let disk = |free: f64, total: f64| Disk { mount: "/".into(), free: gb(free), total: gb(total) };
    for (free, total, want) in [(200.0, 1000.0, None), (199.0, 1000.0, Some(Warn)), (100.0, 1000.0, Some(Warn)), (99.0, 1000.0, Some(Crit)), (50.0, 60.0, None), (49.0, 60.0, Some(Warn)), (20.0, 60.0, Some(Warn)), (19.0, 60.0, Some(Crit))] {
        assert_eq!(level(os(Os::Linux, &|d| d.disks = vec![disk(free, total)]), Metric::Disk), Some(want), "disk {free} free of {total} GB");
    }
    // Each filesystem listed counts: the worst decides; one of no size is not judged.
    let s = os(Os::Darwin, &|d| d.disks = vec![Disk { mount: "/home".into(), free: 0, total: 0 }, disk(500.0, 1000.0), Disk { mount: "/data".into(), ..disk(10.0, 1000.0) }]);
    assert_eq!(s.worst_disk(&t.disk).map(|d| d.mount.as_str()), Some("/data"));
    assert_eq!(s.reading(Metric::Disk, &t), Some((99.0, Some(Crit))));
    // What a machine does not have is not judged (a value of none, not a level of none).
    assert_eq!(os(Os::Linux, &|_| ()).reading(Metric::Compressor, &t), None);
}

/// One of the memory metrics judged by the default thresholds, as `judge` does.
fn judged(samples: &[Sample], metric: Metric, raised: Option<(i64, Level)>) -> Option<(i64, Level)> {
    let t = Thresholds::default();
    evaluate(samples, &|s| s.reading(metric, &t).map(|r| r.1), t.minutes(metric), raised, true, STEP_MS)
}

#[test]
fn the_compressor_alerts_on_the_sample_that_sees_it_and_rises_in_place() {
    use Level::{Crit, Warn};
    let at = |k: i64, v: f32| with(k * STEP_MS, Detail { compressor: Some(v), ..Detail::default() });
    assert_eq!(judged(&[at(0, 10.0), at(1, 61.0)], Metric::Compressor, None), Some((STEP_MS, Warn)), "one sample is enough");
    assert_eq!(judged(&[at(0, 10.0), at(1, 85.0)], Metric::Compressor, None), Some((STEP_MS, Crit)));
    // Rising a level keeps its since.
    assert_eq!(judged(&[at(0, 10.0), at(1, 61.0), at(2, 85.0)], Metric::Compressor, Some((STEP_MS, Warn))), Some((STEP_MS, Crit)));
    // It steps down after a minute below critical, and clears after a minute below warning.
    let mut s = vec![at(0, 85.0)];
    s.extend((1..=11).map(|k| at(k, 70.0)));
    assert_eq!(judged(&s, Metric::Compressor, Some((0, Crit))), Some((0, Crit)), "55 s below");
    s.push(at(12, 70.0));
    assert_eq!(judged(&s, Metric::Compressor, Some((0, Crit))), Some((0, Warn)));
    s.extend((13..=24).map(|k| at(k, 10.0)));
    assert_eq!(judged(&s, Metric::Compressor, Some((0, Warn))), None);
    // A spike while critical is still critical: no step down to rise again.
    assert_eq!(judged(&[at(0, 90.0), at(1, 10.0)], Metric::Compressor, Some((0, Crit))), Some((0, Crit)));
}

#[test]
fn swap_rises_a_level_once_the_window_holds_it() {
    use Level::{Crit, Warn};
    let gb = |g: f32| (g * GIB) as u64;
    let at = |k: i64, g: f32| with(k * STEP_MS, Detail { swap_used: Some(gb(g)), ..Detail::default() });
    let mut s: Vec<Sample> = (0..11).map(|k| at(k, 5.0)).collect();
    assert_eq!(judged(&s, Metric::Swap, None), None, "55 s of a minute");
    s.push(at(11, 5.0));
    assert_eq!(judged(&s, Metric::Swap, None), Some((0, Warn)));
    // Critical for half a minute: still warn; for a minute: critical, the same alert.
    s.extend((12..18).map(|k| at(k, 12.0)));
    assert_eq!(judged(&s, Metric::Swap, Some((0, Warn))), Some((0, Warn)));
    s.extend((18..24).map(|k| at(k, 12.0)));
    assert_eq!(judged(&s, Metric::Swap, Some((0, Warn))), Some((0, Crit)));
    // Straight to critical when the whole minute is.
    assert_eq!(judged(&(0..12).map(|k| at(k, 12.0)).collect::<Vec<_>>(), Metric::Swap, None), Some((0, Crit)));
}

#[test]
fn an_oom_kill_is_critical_at_once_and_held_five_minutes() {
    let at = |secs: i64, kills: u64| Sample { oom_kills_watched: Some(kills), ..with(secs * 1000, Detail::default()) };
    let mut s: Vec<Sample> = (0..10).map(|k| at(k * 5, 0)).collect();
    assert_eq!(evaluate_oom(&s, 5, None, true), None);
    s.push(at(50, 1));
    assert_eq!(evaluate_oom(&s, 5, None, true), Some((50_000, Level::Crit)), "on the sample that sees it");
    s.extend((11..70).map(|k| at(k * 5, 1)));
    assert_eq!(s.last().unwrap().at, 345_000);
    assert_eq!(evaluate_oom(&s, 5, Some((50_000, Level::Crit)), true), Some((50_000, Level::Crit)), "4 min 55 s after the kill");
    let mut cleared = s.clone();
    cleared.push(at(350, 1));
    assert_eq!(evaluate_oom(&cleared, 5, Some((50_000, Level::Crit)), true), None, "5 minutes after the last kill");
    // Another kill during the hold starts the five minutes again; the alert stays the same.
    s.push(at(350, 3));
    s.extend((71..120).map(|k| at(k * 5, 3)));
    assert_eq!(evaluate_oom(&s, 5, Some((50_000, Level::Crit)), true), Some((50_000, Level::Crit)));
    // While the machine is away it neither rises nor clears.
    assert_eq!(evaluate_oom(&cleared, 5, Some((50_000, Level::Crit)), false), Some((50_000, Level::Crit)));
    assert_eq!(evaluate_oom(&s[..11], 5, None, false), None);
}

#[test]
fn thresholds_are_checked_level_by_level() {
    let bad = |f: &dyn Fn(&mut Thresholds), says: &str| {
        let mut t = Thresholds::default();
        f(&mut t);
        let e = t.check().unwrap_err();
        assert!(e.contains(says), "{e}");
    };
    bad(&|t| t.compressor.minutes = Some(1), "compressor");
    bad(&|t| t.swap.crit = 3.0, "swap");
    bad(&|t| t.swap.minutes = None, "swap");
    bad(&|t| t.available.crit = 30.0, "available");
    bad(&|t| t.system_free.warn = 101.0, "system free");
    bad(&|t| t.psi.minutes = Some(11), "PSI");
    bad(&|t| t.oom.minutes = 0, "OOM");
    bad(&|t| t.disk.crit = 70.0, "disk");
    bad(&|t| t.disk.crit_free = 60.0, "disk");
    let t = Thresholds { swap: Levels { on: false, warn: 1.0, crit: 1.0, minutes: Some(10) }, ..Thresholds::default() };
    assert!(t.check().is_ok());
}

#[test]
fn an_older_settings_file_keeps_its_machines_and_takes_the_new_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let old = r#"{"machines":{"box":{"ssh":"box","enabled":true}},"alerts":{"cpu":{"on":false,"percent":80,"minutes":2},"memory":{"on":true,"percent":95,"minutes":10}}}"#;
    std::fs::write(dir.path().join("settings.json"), old).unwrap();
    let t = crate::runtime::Settings::thresholds(dir.path());
    let (cpu, memory) = (Limit { on: false, percent: 80, minutes: 2 }, Limit { on: true, percent: 95, minutes: 10 });
    assert_eq!(t, Thresholds { cpu, memory, ..Thresholds::default() });
    crate::runtime::Settings::set_thresholds(dir.path(), t).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.path().join("settings.json")).unwrap()).unwrap();
    assert_eq!(saved["machines"]["box"]["ssh"], "box", "the machines stay");
    assert_eq!(saved["alerts"]["compressor"], serde_json::json!({ "on": true, "warn": 60.0, "crit": 80.0 }), "no minutes: at once");
    assert_eq!(saved["alerts"]["disk"]["warnFree"], 50.0);
}

#[test]
fn reads_the_processes_of_either_system() {
    let mac = parse_ps(PS_DARWIN);
    assert_eq!(mac.len(), 8);
    assert_eq!(mac[3], (6567, 6287, 783_504 * 1024, "claude".to_string()));
    assert_eq!(mac[4].3, "Google Chrome Helper (Renderer)", "the last part of the path, spaces and all");
    assert_eq!(mac[2].3, "-zsh");
    let linux = parse_ps(PS_LINUX);
    assert_eq!(linux.iter().map(|p| (p.0, p.1, p.3.as_str())).collect::<Vec<_>>(), [(1, 0, "sh"), (14, 1, "sh"), (15, 14, "cat"), (7, 1, "sleep"), (31, 1, "tmux: server")]);
    assert!(parse_ps("not a line\n12 x 3 y\n").is_empty());
}

/// A runtime that knows only its agents and the processes at their roots.
struct Pids(Vec<(RuntimeAgent, Vec<u32>)>);

impl Runtime for Pids {
    fn agents(&self) -> Result<Vec<RuntimeAgent>, workriver_core::runtime::RuntimeError> {
        Ok(self.0.iter().map(|a| a.0.clone()).collect())
    }
    fn locate(&self, _: Option<&str>, _: &str) -> Result<workriver_core::runtime::Place, workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
    fn focus(&self, _: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
    fn start_temporary(&self, _: &str, _: &str, _: &str, _: &str) -> Result<RuntimeAgent, workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
    fn close(&self, _: &str) -> Result<(), workriver_core::runtime::RuntimeError> {
        Err(workriver_core::runtime::RuntimeError::None)
    }
    fn processes(&self, _: Option<&str>) -> Result<Vec<(RuntimeAgent, Vec<u32>)>, workriver_core::runtime::RuntimeError> {
        Ok(self.0.clone())
    }
}

fn agent(name: &str) -> RuntimeAgent {
    let group = Some(workriver_core::RuntimeRef { id: "w1".into(), label: "chat".into(), machine: None });
    RuntimeAgent { session: format!("s-{name}"), harness: "claude".into(), state: workriver_core::runtime::AgentState::Working, cwd: String::new(), group, name: name.into(), topic: None, machine: None }
}

#[test]
fn the_largest_processes_are_named_with_the_agent_they_descend_from() {
    // The agent's shell is 6287; claude (6567) is its child. Another agent's shell is not
    // running anything here.
    let agents = [(agent("reviewer"), vec![6287]), (agent("idle"), vec![9999])];
    let top = top_of(&parse_ps(PS_DARWIN), &agents);
    let rows: Vec<_> = top.iter().map(|p| (p.pid, p.name.as_str(), p.agent.as_deref(), p.place.as_deref())).collect();
    assert_eq!(
        rows,
        [
            (57934, "Google Chrome Helper (Renderer)", None, None),
            (6567, "claude", Some("reviewer"), Some("chat")),
            (1354, "Google Chrome", None, None),
            (1431, "Spotlight", None, None),
            (522, "logd", None, None),
        ]
    );
    assert_eq!(top[1].rss, 783_504 * 1024);
    // A loop in the parents (read while processes came and went) ends.
    let looped = [(10, 11, 1, "a".to_string()), (11, 10, 1, "b".to_string())];
    assert_eq!(top_of(&looped, &agents)[0].agent, None);
}

/// An ssh stand-in that answers the probe with `probe` and `PS` with `ps`, and writes a
/// line to the returned file each time it is asked for the processes.
fn answering(dir: &std::path::Path, name: &str, probe: &str, ps: &str) -> (scan::Runner, PathBuf) {
    let (p, q, log) = (dir.join(format!("{name}.probe")), dir.join(format!("{name}.ps")), dir.join(format!("{name}.log")));
    std::fs::write(&p, probe).unwrap();
    std::fs::write(&q, ps).unwrap();
    let program = dir.join(name);
    std::fs::write(&program, format!("#!/bin/sh\ncase \"$*\" in *'ps -A'*) echo ps >> '{}'; cat '{}';; *) cat '{}';; esac\n", log.display(), q.display(), p.display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    (scan::Runner::Ssh { program: program.to_string_lossy().into(), options: vec![], target: name.into() }, log)
}

#[test]
fn the_processes_are_read_only_when_an_alert_rises() {
    let dir = tempfile::tempdir().unwrap();
    let local: Arc<dyn Runtime> = Arc::new(Pids(vec![]));
    // A followed machine whose own runtime has an agent in the shell 6287.
    let all = Machines::with(local.clone(), vec![("box", Arc::new(Pids(vec![(agent("reviewer"), vec![6287])])))]);
    let (tx, mut rx) = broadcast::channel(16);
    let m = Metrics::new(dir.path().to_path_buf(), printing(dir.path(), "here", DARWIN), Some(all.clone()), all.clone(), tx, Duration::from_secs(5));
    let asked = |log: &PathBuf| std::fs::read_to_string(log).map(|t| t.lines().count()).unwrap_or(0);
    // Calm: the compressor at 17 %. Samples come within a second here, so nothing that
    // needs a minute rises.
    let (calm, log) = answering(dir.path(), "calm", DARWIN, PS_DARWIN);
    all.set_runner("box", calm);
    for _ in 0..3 {
        m.sample("box");
    }
    assert_eq!(asked(&log), 0, "no alert, no process list");
    assert!(m.alerts().is_empty() && rx.try_recv().is_err());
    // The compressor at 66 %: warn at once, and the processes are read once.
    let (warm, log) = answering(dir.path(), "warm", &DARWIN.replace("total: 279400", "total: 1100000"), PS_DARWIN);
    all.set_runner("box", warm);
    m.sample("box");
    assert_eq!(asked(&log), 1);
    assert!(rx.try_recv().is_ok(), "the page hears of it");
    let a = m.alerts();
    assert_eq!((a.len(), a[0].metric, a[0].level, a[0].threshold, a[0].minutes), (1, Metric::Compressor, Level::Warn, Some(60.0), None));
    assert_eq!((a[0].top.len(), a[0].top[1].agent.as_deref(), a[0].top[1].place.as_deref()), (5, Some("reviewer"), Some("chat")), "{:?}", a[0].top);
    assert!((a[0].value - 66.78).abs() < 0.01, "{}", a[0].value);
    m.sample("box");
    m.sample("box");
    assert_eq!(asked(&log), 1, "the same level again: not read again");
    assert!(rx.try_recv().is_err(), "nothing changed, nothing said");
    // 85 %: critical, the same alert, read again.
    let (hot, log) = answering(dir.path(), "hot", &DARWIN.replace("total: 279400", "total: 1400000"), PS_DARWIN);
    all.set_runner("box", hot);
    m.sample("box");
    assert_eq!(asked(&log), 1);
    let b = m.alerts();
    assert_eq!((b[0].level, &b[0].since, b[0].threshold), (Level::Crit, &a[0].since, Some(80.0)));
    assert_eq!(m.report().machines[1].alerts["compressor"].level, Level::Crit);
    assert!(rx.try_recv().is_ok());
    let text = crate::cli::load_text(&serde_json::to_value(m.report()).unwrap());
    assert!(text.contains("ALERT: compressor CRIT since"), "{text}");
}
