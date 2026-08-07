//! Stands in for a published `lattice` binary in the upgrade and handover tests.
//!
//! It prints the file name it was invoked as, followed by the arguments it
//! received. The file name is what makes a handover observable: the versioned
//! install is `lattice-<version>`, so seeing it in the output is proof that the
//! *pinned* build ran rather than the one that was invoked.
//!
//! A program rather than a shell script, so a published release is something
//! both platforms can actually execute.

fn main() {
	// `file_stem` would read the `.9` of `lattice-9.9.9` as an extension and drop
	// it, so only the platform's executable suffix comes off.
	let name = std::env::current_exe()
		.ok()
		.and_then(|p| p.file_name().map(|s| s.to_string_lossy().into_owned()))
		.map(|n| {
			let suffix = std::env::consts::EXE_SUFFIX;
			n.strip_suffix(suffix).map(str::to_string).unwrap_or(n)
		})
		.unwrap_or_else(|| "release-stub".to_string());
	let args: Vec<String> = std::env::args().skip(1).collect();
	println!("{name} {}", args.join(" "));
}
