fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        embed_resource::compile_for("assets/brand/wsm.rc", ["wsm"], embed_resource::NONE);
    }
}
