//! Task commands for Lattice's own tests, written for whichever shell will run
//! them.
//!
//! A task's command goes to the platform shell — `sh -c` on unix, `cmd /C` on
//! Windows — so a test that spells its task body as a shell script is asserting
//! against one shell's grammar. `cmd` has no `;` separator, its `mkdir` takes no
//! `-p`, and `echo hi > f` writes a trailing space. A suite written in POSIX
//! shell therefore does not test the same thing on Windows; it fails for reasons
//! that have nothing to do with what it covers.
//!
//! Every function here returns a command string that does the same *observable*
//! thing on both platforms, so a test can say what the task should do and stay
//! out of the shell's way.
//!
//! ```ignore
//! // "write hi to out.txt, then fail"
//! let cmd = testkit::all(&[testkit::write("out.txt", "hi"), testkit::exit(1)]);
//! ```
//!
//! Paths are spelled with the platform's separator and quoted, so a temp
//! directory with a space in it survives. Use [`json`] when the command has to
//! be embedded in a `lattice.json` written as text: a Windows path is full of
//! backslashes, and `"dist\out.txt"` is not valid JSON.

use std::path::Path;

/// Whether the command will be handed to `cmd` rather than a POSIX shell.
const CMD: bool = cfg!(windows);

/// A path spelled for the platform shell: native separators, quoted.
pub fn path(p: impl AsRef<Path>) -> String {
	let text = p.as_ref().display().to_string();
	if CMD {
		format!("\"{}\"", text.replace('/', "\\"))
	} else {
		format!("'{text}'")
	}
}

/// A [`std::process::Command`] that runs `command` through the platform shell,
/// the way Lattice itself hands a task over.
///
/// On Windows the command goes in as a raw `/S /C "<command>"` argument rather
/// than through `.arg()`. Rust quotes arguments the way the MSVC runtime parses
/// them, escaping an embedded `"` as `\"`, which `cmd` does not read as an escape
/// — so a command carrying a quote arrives mangled. Anything that spawns a shell
/// needs this, which is why it lives here rather than being written out again at
/// each call site.
pub fn shell_command(command: &str) -> std::process::Command {
	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		let mut c = std::process::Command::new("cmd");
		c.raw_arg(format!("/S /C \"{command}\""));
		c
	}
	#[cfg(not(windows))]
	{
		let mut c = std::process::Command::new("sh");
		c.arg("-c").arg(command);
		c
	}
}

/// A command string escaped for embedding in JSON, quotes included.
///
/// A Windows path is full of backslashes, so pasting one into a `lattice.json`
/// written as a raw string produces invalid JSON rather than the command.
pub fn json(cmd: &str) -> String {
	serde_json::to_string(cmd).expect("a command string always serializes")
}

/// Text that is safe to hand to `echo`/`printf` unquoted-ish. Loud rather than
/// silently wrong: a metacharacter would need per-shell escaping, and no test
/// here needs one.
fn check_literal(text: &str) {
	const SPECIAL: &[char] = &[
		'&', '|', '<', '>', '^', '%', '"', '\'', '`', '$', '\\', '\n',
	];
	assert!(
		!text.contains(SPECIAL),
		"testkit: {text:?} contains a shell metacharacter; add explicit escaping \
		 rather than relying on either shell's defaults"
	);
}

/// A command that succeeds and does nothing.
pub fn succeed() -> String {
	if CMD {
		"cd .".into()
	} else {
		"true".into()
	}
}

/// A command that exits with `code`.
pub fn exit(code: i32) -> String {
	format!("exit {code}")
}

/// Print `text` and a newline to stdout.
pub fn echo(text: &str) -> String {
	check_literal(text);
	if CMD {
		format!("echo {text}")
	} else {
		format!("printf '%s\\n' '{text}'")
	}
}

/// Print `text` and a newline to stderr.
pub fn echo_err(text: &str) -> String {
	check_literal(text);
	if CMD {
		format!("(echo {text})>&2")
	} else {
		format!("printf '%s\\n' '{text}' >&2")
	}
}

/// Write `text` and a newline to `path`, replacing what was there.
///
/// The `cmd` form parenthesizes the `echo`, which keeps the redirection from
/// binding to it: `echo seed1>f` reads the `1` as a file descriptor, and
/// `echo hi >f` writes a trailing space.
pub fn write(p: impl AsRef<Path>, text: &str) -> String {
	check_literal(text);
	if CMD {
		format!("(echo {text})>{}", path(p))
	} else {
		format!("printf '%s\\n' '{text}' > {}", path(p))
	}
}

/// Append `text` and a newline to `path`.
pub fn append(p: impl AsRef<Path>, text: &str) -> String {
	check_literal(text);
	if CMD {
		format!("(echo {text})>>{}", path(p))
	} else {
		format!("printf '%s\\n' '{text}' >> {}", path(p))
	}
}

/// Create an empty file at `path`.
pub fn touch(p: impl AsRef<Path>) -> String {
	if CMD {
		format!("type nul>{}", path(p))
	} else {
		format!(": > {}", path(p))
	}
}

/// Copy `src`'s bytes to `dst`. Neither form rewrites line endings.
pub fn copy(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> String {
	if CMD {
		format!("type {}>{}", path(src), path(dst))
	} else {
		format!("cat {} > {}", path(src), path(dst))
	}
}

/// Create `dir` and any missing parents. Succeeds if it already exists.
pub fn mkdirs(dir: impl AsRef<Path>) -> String {
	let quoted = path(&dir);
	if CMD {
		// `mkdir` creates intermediates on its own here, but fails on an existing
		// directory, so the existence check is what makes this idempotent.
		//
		// The parentheses are load-bearing: in `if not exist X mkdir X && Y`, the
		// `&& Y` binds inside the `if`, so `Y` is skipped whenever the directory
		// already exists. Grouping the `if` keeps the rest of an [`all`] chain out
		// of the condition.
		format!("(if not exist {quoted} mkdir {quoted})")
	} else {
		format!("mkdir -p {quoted}")
	}
}

/// Sleep for `secs` seconds.
///
/// `timeout` is the obvious `cmd` answer and the wrong one: it reads the console
/// directly and fails outright when stdin is a pipe, which is how a task's shell
/// is always spawned. Pinging the loopback the right number of times is the form
/// that works unattended.
pub fn sleep(secs: u64) -> String {
	if CMD {
		format!("ping -n {} 127.0.0.1 >nul", secs.saturating_add(1))
	} else {
		format!("sleep {secs}")
	}
}

/// A character that [`std::env::join_paths`] refuses, for a test that needs a
/// `PATH` entry which cannot be joined.
///
/// Not the separator. Unix splits `PATH` on `:` and rejects an entry containing
/// one, so there the two are the same character. Windows splits on `;` but
/// quotes an entry that contains one rather than refusing it, and rejects `"`
/// instead. A test that reaches for the separator on both therefore asserts
/// nothing on Windows: the join succeeds and the error never arrives.
pub fn unjoinable_char() -> char {
	if CMD {
		'"'
	} else {
		':'
	}
}

/// A command that leaves behind a background process which ignores `SIGTERM`
/// and keeps the task's stdout open, then exits itself.
///
/// Unix only, and deliberately so: the leftover is what proves a runner
/// escalates from asking a process group to stop to killing it. Windows has no
/// process group here — a task's tree is taken down directly — so there is no
/// equivalent situation to set up, and a stand-in that merely slept would assert
/// nothing. Gate the test on `#[cfg(unix)]` rather than reaching for a
/// cross-platform spelling that does not exist.
#[cfg(unix)]
pub fn stubborn_background(secs: u64) -> String {
	format!("(trap '' TERM; sleep {secs}) & sleep {secs}")
}

/// A command that leaves behind a background process in a process group of its
/// own, still holding the task's stdout.
///
/// `set -m` is what puts the job in its own group — POSIX job control, and the
/// only spelling of this that needs no interpreter a test cannot assume is
/// installed. The escape is the whole point: a leftover inside the task's group
/// is reached by the runner's kill, so it proves nothing about the case where
/// the pipe outlives every process the runner can name. `tauri dev` starting its
/// `beforeDevCommand` in a fresh group is the shape this stands in for.
///
/// Unix only, for the reason [`stubborn_background`] gives.
#[cfg(unix)]
pub fn escaped_background(secs: u64) -> String {
	format!("set -m; sleep {secs} & sleep {secs}")
}

/// A command that prints `text`, then a byte that is not valid UTF-8, then
/// `after` — each on its own line.
///
/// Unix only. `cmd` has no way to emit an arbitrary raw byte without an
/// interpreter no test can assume is installed, and a version that emitted
/// valid text on Windows would silently stop testing the thing it exists for:
/// that a task's output survives a byte the runner cannot decode.
#[cfg(unix)]
pub fn echo_invalid_utf8(text: &str, after: &str) -> String {
	check_literal(text);
	check_literal(after);
	format!("printf '{text} \\377 more\\n{after}\\n'")
}

/// Run each command in order, stopping at the first failure.
pub fn all<I, S>(cmds: I) -> String
where
	I: IntoIterator<Item = S>,
	S: AsRef<str>,
{
	join(cmds, " && ")
}

/// Run each command in order whether or not the previous one succeeded.
///
/// [`exit`] ends the shell in both grammars, so it stops the sequence wherever it
/// appears. It belongs last.
pub fn then<I, S>(cmds: I) -> String
where
	I: IntoIterator<Item = S>,
	S: AsRef<str>,
{
	join(cmds, if CMD { " & " } else { " ; " })
}

fn join<I, S>(cmds: I, sep: &str) -> String
where
	I: IntoIterator<Item = S>,
	S: AsRef<str>,
{
	cmds.into_iter()
		.map(|c| c.as_ref().to_string())
		.collect::<Vec<_>>()
		.join(sep)
}

/// An `installCmd` that provisions a stand-in tool which prints
/// `<name> <version>` when run, into the toolchain's `bin` directory.
///
/// `$LATTICE_TOOLCHAIN_DIR` is substituted into the string before the shell sees
/// it, so the same placeholder works on both platforms. On Windows the tool is a
/// `.cmd` file, which is what makes a bare `name` on `PATH` resolve to it.
pub fn install_fake_tool(name: &str, version: &str) -> String {
	check_literal(name);
	check_literal(version);
	if CMD {
		let bin = "\"$LATTICE_TOOLCHAIN_DIR\\bin\"";
		let tool = format!("\"$LATTICE_TOOLCHAIN_DIR\\bin\\{name}.cmd\"");
		format!("if not exist {bin} mkdir {bin} && (echo @echo {name} {version})>{tool}")
	} else {
		let bin = "\"$LATTICE_TOOLCHAIN_DIR/bin\"";
		let tool = format!("\"$LATTICE_TOOLCHAIN_DIR/bin/{name}\"");
		format!(
			"mkdir -p {bin} && printf '#!/bin/sh\\necho {name} {version}\\n' > {tool} \
			 && chmod +x {tool}"
		)
	}
}

/// The file name the stand-in tool from [`install_fake_tool`] is installed as.
pub fn fake_tool_file(name: &str) -> String {
	if CMD {
		format!("{name}.cmd")
	} else {
		name.to_string()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Every builder has to produce something the platform shell will actually
	/// run, so each is executed and its effect checked rather than its text.
	///
	/// Through [`shell_command`], which is the same door Lattice uses — a harness
	/// that spawned the shell its own way could pass while the real thing failed.
	fn run(cmd: &str, dir: &Path) -> std::process::Output {
		let mut c = shell_command(cmd);
		c.current_dir(dir);
		c.output().expect("spawn the platform shell")
	}

	/// A directory of its own per call. Tests in one binary share a process, so a
	/// name built from the pid alone would have them clearing each other's files.
	fn tmp() -> std::path::PathBuf {
		use std::sync::atomic::{AtomicUsize, Ordering};
		static NEXT: AtomicUsize = AtomicUsize::new(0);
		let dir = std::env::temp_dir().join(format!(
			"lattice-testkit-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		let _ = std::fs::remove_dir_all(&dir);
		std::fs::create_dir_all(&dir).unwrap();
		dir
	}

	#[test]
	fn succeed_and_exit_report_their_status() {
		let dir = tmp();
		assert!(run(&succeed(), &dir).status.success());
		assert!(!run(&exit(1), &dir).status.success());
		assert_eq!(run(&exit(3), &dir).status.code(), Some(3));
	}

	#[test]
	fn write_replaces_and_append_adds() {
		let dir = tmp();
		assert!(run(&write("out.txt", "first"), &dir).status.success());
		assert_eq!(
			std::fs::read_to_string(dir.join("out.txt")).unwrap().trim(),
			"first"
		);

		assert!(run(&write("out.txt", "second"), &dir).status.success());
		assert_eq!(
			std::fs::read_to_string(dir.join("out.txt")).unwrap().trim(),
			"second",
			"write must replace, not append"
		);

		assert!(run(&append("out.txt", "third"), &dir).status.success());
		let lines: Vec<String> = std::fs::read_to_string(dir.join("out.txt"))
			.unwrap()
			.lines()
			.map(str::to_string)
			.collect();
		assert_eq!(lines, vec!["second", "third"]);
	}

	/// `echo seed1>f` reads the trailing digit as a file descriptor under `cmd`,
	/// which silently drops it from the file.
	#[test]
	fn write_keeps_a_trailing_digit() {
		let dir = tmp();
		assert!(run(&write("n.txt", "seed1"), &dir).status.success());
		assert_eq!(
			std::fs::read_to_string(dir.join("n.txt")).unwrap().trim(),
			"seed1"
		);
	}

	#[test]
	fn echo_goes_to_stdout_and_echo_err_to_stderr() {
		let dir = tmp();
		let out = run(&echo("ON-STDOUT"), &dir);
		assert!(String::from_utf8_lossy(&out.stdout).contains("ON-STDOUT"));

		let err = run(&echo_err("ON-STDERR"), &dir);
		assert!(String::from_utf8_lossy(&err.stderr).contains("ON-STDERR"));
	}

	#[test]
	fn touch_creates_an_empty_file() {
		let dir = tmp();
		assert!(run(&touch("marker"), &dir).status.success());
		assert_eq!(std::fs::metadata(dir.join("marker")).unwrap().len(), 0);
	}

	#[test]
	fn copy_reproduces_the_bytes() {
		let dir = tmp();
		std::fs::write(dir.join("src.json"), r#"{"mode":"one"}"#).unwrap();
		assert!(run(&copy("src.json", "dst.json"), &dir).status.success());
		assert_eq!(
			std::fs::read_to_string(dir.join("dst.json")).unwrap(),
			r#"{"mode":"one"}"#,
			"a copy must not add or rewrite anything"
		);
	}

	#[test]
	fn mkdirs_creates_parents_and_is_idempotent() {
		let dir = tmp();
		assert!(run(&mkdirs("a/b/c"), &dir).status.success());
		assert!(dir.join("a/b/c").is_dir());
		assert!(
			run(&mkdirs("a/b/c"), &dir).status.success(),
			"running it again must not fail"
		);
	}

	/// The second run is the one that matters. Under `cmd`, `&&` after a bare `if`
	/// binds inside the condition, so once the directory exists everything after
	/// `mkdirs` in the chain silently stops running — which looks exactly like a
	/// task that succeeded and did nothing.
	#[test]
	fn mkdirs_does_not_swallow_the_rest_of_the_chain() {
		let dir = tmp();
		let cmd = all([mkdirs("dist"), write("dist/after.txt", "after")]);

		assert!(run(&cmd, &dir).status.success());
		assert!(dir.join("dist/after.txt").exists(), "first run");

		std::fs::remove_file(dir.join("dist/after.txt")).unwrap();
		assert!(run(&cmd, &dir).status.success());
		assert!(
			dir.join("dist/after.txt").exists(),
			"the directory already existed, and the rest of the chain still has to run"
		);
	}

	#[test]
	fn all_stops_at_the_first_failure() {
		let dir = tmp();
		let cmd = all([exit(1), write("unreachable.txt", "x")]);
		assert!(!run(&cmd, &dir).status.success());
		assert!(!dir.join("unreachable.txt").exists());
	}

	/// A failing command, not [`exit`], which ends the shell in both grammars and
	/// so would stop the sequence for a reason that says nothing about `then`.
	#[test]
	fn then_carries_on_past_a_failure() {
		let dir = tmp();
		let failing = copy("no-such-file.txt", "ignored.txt");
		assert!(!run(&failing, &dir).status.success(), "the premise");

		let cmd = then([failing, write("reached.txt", "x")]);
		run(&cmd, &dir);
		assert!(
			dir.join("reached.txt").exists(),
			"the second command must run anyway"
		);
	}

	#[test]
	fn all_runs_each_command_in_order() {
		let dir = tmp();
		let cmd = all([
			mkdirs("dist"),
			write("dist/one.txt", "one"),
			append("dist/one.txt", "two"),
		]);
		assert!(run(&cmd, &dir).status.success());
		let lines: Vec<String> = std::fs::read_to_string(dir.join("dist/one.txt"))
			.unwrap()
			.lines()
			.map(str::to_string)
			.collect();
		assert_eq!(lines, vec!["one", "two"]);
	}

	#[test]
	fn sleep_actually_waits() {
		let dir = tmp();
		let started = std::time::Instant::now();
		assert!(run(&sleep(1), &dir).status.success());
		assert!(
			started.elapsed() >= std::time::Duration::from_millis(700),
			"sleep returned in {:?}, so it is not waiting",
			started.elapsed()
		);
	}

	#[test]
	fn json_escapes_a_command_for_embedding() {
		let quoted = json(r#"a\b "c""#);
		assert!(quoted.starts_with('"') && quoted.ends_with('"'));
		let back: String = serde_json::from_str(&quoted).unwrap();
		assert_eq!(back, r#"a\b "c""#);
	}

	#[test]
	fn a_metacharacter_is_rejected_rather_than_mangled() {
		let caught = std::panic::catch_unwind(|| echo("a && b"));
		assert!(caught.is_err(), "a metacharacter must not pass silently");
	}
}
