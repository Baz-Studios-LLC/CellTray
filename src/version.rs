pub const BUILD: u32 = 4;
pub fn label() -> String {
    format!("v{BUILD:03}")
}
