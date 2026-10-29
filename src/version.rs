pub const BUILD: u32 = 7;
pub fn label() -> String {
    format!("v{BUILD:03}")
}
