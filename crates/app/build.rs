//! Embeds the icon and version information Explorer, shortcuts and Settings >
//! Apps show for `saw-app.exe`. Off Windows there is nothing to embed into.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut resource = winresource::WindowsResource::new();
    resource
        .set_icon("assets/saw-auto-clicker.ico")
        .set("ProductName", "saw-auto-clicker")
        .set("FileDescription", "saw-auto-clicker");
    resource
        .compile()
        .expect("the icon and version information compile into a resource");
}
