pub const BUILD: u32 = 1;
pub fn label() -> String {
    format!("v{BUILD:03}")
}
