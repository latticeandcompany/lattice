fn main() {
	// `std::env::consts::ARCH` is only the architecture. The installer and bug
	// reports both need the full target triple, which is only knowable here.
	println!(
		"cargo::rustc-env=LATTICE_TARGET={}",
		std::env::var("TARGET").expect("cargo always sets TARGET for build scripts")
	);
	println!("cargo::rerun-if-changed=build.rs");
}
