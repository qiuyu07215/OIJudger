//! Maintenance: delete AppContainer profiles by name.
//! Usage: delete_profiles <name>...
fn main() {
    #[cfg(windows)]
    for n in std::env::args().skip(1) {
        sandbox::Container::delete_profile(&n);
        println!("deleted {n}");
    }
}
