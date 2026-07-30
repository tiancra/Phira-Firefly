fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();

        // 填写 relative 路径（相对于 workspace 根路径或当前 crate 路径）
        res.set_icon("../assets/icon.ico");

        res.compile().unwrap();
    }
}
