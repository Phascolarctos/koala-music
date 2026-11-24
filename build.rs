fn main() {
    slint_build::compile("ui/app-window.slint").expect("Slint build failed");
    // UniFFI：根据 UDL 生成跨语言脚手架（供 Kotlin 绑定使用）
    uniffi::generate_scaffolding("src/koala.udl").expect("UniFFI scaffolding failed");
}
