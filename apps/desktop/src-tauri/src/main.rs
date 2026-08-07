// The window is the interface; a console window behind it on Windows is not.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
	lattice_desktop_lib::run();
}
