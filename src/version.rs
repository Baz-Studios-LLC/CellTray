pub const BUILD: u32 = 5;
pub fn label() -> String {
    format!("v{BUILD:03}")
}
