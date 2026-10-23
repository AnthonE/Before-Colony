//! Writes the page's gallery (`bc_client_core::gallery`) as JSON, for `web/gallery.js`:
//! `cargo run -q -p bc-client-core --example page_gallery > web/dist/gallery.json`
//! (`scripts/build-web.sh` does it).

fn main() {
    let json = serde_json::to_string(&bc_client_core::gallery::gallery()).expect("the gallery is JSON");
    println!("{json}");
}
