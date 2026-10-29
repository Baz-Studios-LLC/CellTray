pub const BUILD: u32 = 3;
pub fn label() -> String {
    format!("v{BUILD:03}")
}
