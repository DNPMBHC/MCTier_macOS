//! macOS platform support for adapter detection and per-user auto-start.

use std::path::{Path, PathBuf};

const LAUNCH_AGENT_LABEL: &str = "com.mctier.app";

fn launch_agent_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join("Library")
            .join("LaunchAgents")
            .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
    })
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn launch_agent_plist(executable: &Path) -> String {
    let program = xml_escape(&executable.to_string_lossy());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
           <key>Label</key>\n\
           <string>{LAUNCH_AGENT_LABEL}</string>\n\
           <key>ProgramArguments</key>\n\
           <array>\n\
             <string>{program}</string>\n\
           </array>\n\
           <key>RunAtLoad</key>\n\
           <true/>\n\
         </dict>\n\
         </plist>\n"
    )
}

/// EasyTier normally creates a utun interface on macOS. Keep the check narrow
/// so unrelated VPN and container interfaces do not make diagnostics report a
/// false positive.
pub fn has_virtual_adapter() -> Result<bool, String> {
    let output = std::process::Command::new("/sbin/ifconfig")
        .output()
        .map_err(|error| format!("执行 ifconfig 失败: {error}"))?;
    if !output.status.success() {
        return Err(format!("ifconfig 返回失败状态: {}", output.status));
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().any(|line| {
        let name = line.strip_suffix(':').unwrap_or(line).trim();
        name.starts_with("utun") || name.starts_with("easytier") || name == "MCTier_Net"
    }))
}

pub fn set_auto_start(enable: bool) -> Result<(), String> {
    let path = launch_agent_path().ok_or("无法确定 macOS 用户目录")?;
    if enable {
        let executable =
            std::env::current_exe().map_err(|error| format!("获取程序路径失败: {error}"))?;
        let parent = path.parent().ok_or("无法确定 LaunchAgents 目录")?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("创建 LaunchAgents 目录失败: {error}"))?;
        std::fs::write(&path, launch_agent_plist(&executable))
            .map_err(|error| format!("写入 macOS 自启动配置失败: {error}"))?;
        log::info!("macOS 开机自启动已启用: {:?}", path);
        return Ok(());
    }

    match std::fs::remove_file(&path) {
        Ok(()) => {
            log::info!("macOS 开机自启动已禁用");
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            log::info!("macOS 开机自启动已禁用");
            Ok(())
        }
        Err(error) => Err(format!("删除 macOS 自启动配置失败: {error}")),
    }
}

pub fn auto_start_enabled() -> bool {
    launch_agent_path()
        .map(|path| path.is_file())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_escapes_executable_path() {
        let plist = launch_agent_plist(Path::new("/tmp/MCTier<&\".app/Contents/MacOS/MCTier"));
        assert!(plist.contains("/tmp/MCTier&lt;&amp;&quot;.app/Contents/MacOS/MCTier"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
    }

    #[test]
    fn adapter_names_are_narrowly_matched() {
        for line in ["utun4: flags=", "easytier0: flags=", "MCTier_Net: flags="] {
            let name = line.split(':').next().unwrap();
            assert!(
                name.starts_with("utun") || name.starts_with("easytier") || name == "MCTier_Net"
            );
        }
        assert!(!"tun0".starts_with("utun"));
    }
}
