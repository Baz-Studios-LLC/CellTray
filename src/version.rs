pub const BUILD: u32 = 2;
pub fn label() -> String {
    format!("v{BUILD:03}")
}
