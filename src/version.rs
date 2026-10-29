pub const BUILD: u32 = 6;
pub fn label() -> String {
    format!("v{BUILD:03}")
}
