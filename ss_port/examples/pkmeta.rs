//! Debug: print a .pk's meta JSON (nodes with mesh/skin, skins).
//!   cargo run --release --example pkmeta -- <file.pk> [key]
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let f = ss_port::pk::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    match a.get(2) {
        Some(k) => println!("{}", serde_json::to_string_pretty(&f.meta[k.as_str()]).unwrap()),
        None => println!("{:?}", f.meta.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>())),
    }
}
