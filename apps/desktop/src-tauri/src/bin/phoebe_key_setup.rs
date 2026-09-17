fn main() {
    eprintln!(
        "将 DeepSeek API Key 保存到菲比助手专用的系统凭证条目。输入不会显示或写入项目文件。\n"
    );
    let key = match rpassword::prompt_password("DeepSeek API Key: ") {
        Ok(value) => value,
        Err(_) => {
            eprintln!("无法读取输入");
            std::process::exit(1);
        }
    };
    let store = phoebe_desktop_lib::secure_store::SecureStore::default();
    match store.set_key(&key) {
        Ok(()) => eprintln!("已保存到系统安全存储。"),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}
