//! A stand-in for a nested repo's own task runner, for the passthrough tests.
//!
//! `lattice` schedules a runner-owned workspace as one opaque node, so the tests
//! need something on `PATH` that behaves like that runner: fans a task out over
//! the repo's packages, writes their artifacts, and leaves a nondeterministic
//! file in its own cache directory on every invocation — which is what the
//! `ignore` patterns in those tests have to keep out of Lattice's cache key.
//!
//! Written as a program rather than a shell script because a shell script is a
//! shell script: the POSIX version of this could only ever run on unix, and the
//! passthrough pattern is not a unix feature.

use std::io::Write;

fn main() {
	let args: Vec<String> = std::env::args().skip(1).collect();
	if args.first().map(String::as_str) != Some("run") {
		eprintln!("turbo-stub: expected 'run', got '{}'", args.join(" "));
		std::process::exit(2);
	}
	let task = args.get(1).cloned().unwrap_or_default();

	// The inner runner's own cache: different on every invocation, and none of
	// Lattice's business.
	if let Err(e) = std::fs::create_dir_all(".turbo") {
		fail(&format!("could not create .turbo: {e}"));
	}
	let stamp = format!(
		"{} {:?}",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map(|d| d.as_nanos())
			.unwrap_or_default()
	);
	if let Err(e) = std::fs::write(".turbo/last-run", stamp) {
		fail(&format!("could not write .turbo/last-run: {e}"));
	}

	// Fan out over the packages, in a stable order so the output is comparable.
	let mut packages: Vec<std::path::PathBuf> = std::fs::read_dir("packages")
		.map(|rd| {
			rd.flatten()
				.map(|e| e.path())
				.filter(|p| p.is_dir())
				.collect()
		})
		.unwrap_or_default();
	packages.sort();

	for pkg in &packages {
		let dist = pkg.join("dist");
		if let Err(e) = std::fs::create_dir_all(&dist) {
			fail(&format!("could not create {}: {e}", dist.display()));
		}
		let src = pkg.join("src").join("index.js");
		let bytes = match std::fs::read(&src) {
			Ok(b) => b,
			Err(e) => fail(&format!("could not read {}: {e}", src.display())),
		};
		if let Err(e) = std::fs::write(dist.join("bundle.js"), bytes) {
			fail(&format!("could not write the bundle: {e}"));
		}
		let name = pkg
			.file_name()
			.map(|n| n.to_string_lossy().into_owned())
			.unwrap_or_default();
		println!("{name}:{task}: done");
	}

	println!("turbo-stub: {task} complete");
	let _ = std::io::stdout().flush();
}

fn fail(message: &str) -> ! {
	eprintln!("turbo-stub: {message}");
	std::process::exit(1);
}
