//! Debug: list nodes / animation tracks of a .pk matching a name.
//!   cargo run --release --example pkdump -- <file.pk> <name-substring>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let f = ss_port::pk::parse(&std::fs::read(&a[1]).unwrap()).unwrap();
    let pat = a.get(2).cloned().unwrap_or_default();
    for (i, n) in f.meta["nodes"].as_array().unwrap().iter().enumerate() {
        let name = n["name"].as_str().unwrap_or("");
        if name.contains(&pat) {
            println!("node {i} {name} children {:?}", n["children"]);
        }
    }
    println!("skin joints {:?}", f.meta["skins"][0]["joints"]);
    for an in f.animations() {
        for t in an.tracks.iter().filter(|t| t.target.contains(&pat)) {
            println!("anim {} track {} {:?} keys {} first {:?}", an.name, t.target, t.channel, t.times.len(), &t.values[..t.channel.size().min(t.values.len())]);
        }
    }
}
