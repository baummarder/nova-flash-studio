//! Generates the M10 game sample projects under `samples/`.
//!
//! Run: `cargo run -p nova-core --example gen_games`

use nova_core::games;

fn main() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("samples");
    std::fs::create_dir_all(&dir).expect("samples dir");
    for template in games::TEMPLATES {
        let project = games::create_project(template.id).expect("template exists");
        let milestone = if template.id.starts_with("tilemap-") || template.id == "particle-showcase"
        {
            "m11"
        } else {
            "m10"
        };
        let path = dir.join(format!("{milestone}-{}.nova", template.id));
        project.save(&path).expect("save project");
        println!(" wrote {}", path.display());
    }
}
