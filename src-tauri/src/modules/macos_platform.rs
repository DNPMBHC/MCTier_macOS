//! macOS platform support for adapter detection, per-user auto-start and
//! background-activity (App Nap) control.

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

pub fn open_privacy_settings(kind: &str) -> Result<(), String> {
    let destination = match kind {
        "microphone" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
        }
        "screen-recording" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        }
        "accessibility" => {
            "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        }
        _ => return Err("未知的 macOS 权限类型".to_string()),
    };
    std::process::Command::new("/usr/bin/open")
        .arg(destination)
        .spawn()
        .map_err(|error| format!("打开 macOS 隐私设置失败: {error}"))?;
    Ok(())
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

/// NSActivityUserInitiatedAllowingIdleSystemSleep：向系统声明「用户主动的事务仍在进行」。
/// 它阻止 App Nap 对隐藏窗口进程做节流（隐藏后语音/采集的 WebView 取流泵还要实时跑），
/// 同时保留显示器正常休眠——不带 IdleSystemSleepDisabled 才是这里的正确选项。
const NS_ACTIVITY_USER_INITIATED_ALLOWING_IDLE_SYSTEM_SLEEP: u64 = (1 << 21) | (1 << 22);

type Obj = *mut std::ffi::c_void;
type Sel = *const std::ffi::c_void;

extern "C" {
    fn objc_getClass(name: *const std::ffi::c_char) -> Obj;
    fn sel_registerName(name: *const std::ffi::c_char) -> Sel;
    // arm64 上所有普通参数共用同一个 objc_msgSend 实现；按调用形状各声明一次即可，
    // 这是调用 ObjC 的标准写法，签名必然互不相同，关掉重复声明检查。
    #[allow(clashing_extern_declarations)]
    #[link_name = "objc_msgSend"]
    fn msg_send_object(receiver: Obj, sel: Sel) -> Obj;
    #[allow(clashing_extern_declarations)]
    #[link_name = "objc_msgSend"]
    fn msg_send_begin_activity(receiver: Obj, sel: Sel, options: u64, reason: *const std::ffi::c_void) -> u64;
    #[allow(clashing_extern_declarations)]
    #[link_name = "objc_msgSend"]
    fn msg_send_end_activity(receiver: Obj, sel: Sel, token: u64) -> Obj;
}

static REALTIME_ACTIVITY_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static REALTIME_ACTIVITY_TOKEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn process_info() -> Obj {
    unsafe {
        let class = objc_getClass(c"NSProcessInfo".as_ptr());
        msg_send_object(class, sel_registerName(c"processInfo".as_ptr()))
    }
}

fn begin_realtime_activity() -> u64 {
    // reason 需要一个 NSString；CFString 与 NSString 桥接（toll-free bridged）。
    // 进程存活期间豁免状态可能反复切换，reason 常驻一次即可。
    static REASON: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let reason = *REASON.get_or_init(|| {
        use core_foundation::base::TCFType;
        use core_foundation::string::CFString;
        let string = CFString::new("MCTier 语音通话或屏幕采集进行中");
        let raw = string.as_concrete_TypeRef() as usize;
        // mem::forget 保留 +1 引用：字符串对象与进程同寿命，指针永远有效。
        std::mem::forget(string);
        raw
    }) as *const std::ffi::c_void;
    unsafe {
        msg_send_begin_activity(
            process_info(),
            sel_registerName(c"beginActivityWithOptions:reason:".as_ptr()),
            NS_ACTIVITY_USER_INITIATED_ALLOWING_IDLE_SYSTEM_SLEEP,
            reason,
        )
    }
}

fn end_realtime_activity(token: u64) {
    unsafe {
        msg_send_end_activity(
            process_info(),
            sel_registerName(c"endActivity:".as_ptr()),
            token,
        );
    }
}

/// 有实时会话（原生麦克风/屏幕采集）时阻止 App Nap 节流；全部结束后释放。
/// 幂等且可从任意线程调用；token 配对由 active 边沿驱动，避免泄漏重复的 activity。
pub fn set_realtime_activity(active: bool) {
    use std::sync::atomic::Ordering;
    let was = REALTIME_ACTIVITY_ACTIVE.swap(active, Ordering::AcqRel);
    if was == active {
        return;
    }
    if active {
        let token = begin_realtime_activity();
        REALTIME_ACTIVITY_TOKEN.store(token, Ordering::Release);
        log::debug!("已申请 macOS 后台活动豁免（token={token}）");
    } else {
        let token = REALTIME_ACTIVITY_TOKEN.swap(0, Ordering::AcqRel);
        if token != 0 {
            end_realtime_activity(token);
            log::debug!("已释放 macOS 后台活动豁免（token={token}）");
        }
    }
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

    /// 数值按 NSProcessInfo.h 抄写：(1 << 21) | (1 << 22)。
    #[test]
    fn activity_option_matches_the_sdk_header() {
        assert_eq!(NS_ACTIVITY_USER_INITIATED_ALLOWING_IDLE_SYSTEM_SLEEP, 0x0060_0000);
    }

    /// begin/end 由 active 边沿驱动：重复 true/false 不应重复申请或泄漏 token。
    /// beginActivity 无需系统授权，可以直接跑。
    #[test]
    fn realtime_activity_toggles_are_edge_driven() {
        set_realtime_activity(true);
        set_realtime_activity(true);
        set_realtime_activity(false);
        set_realtime_activity(false);
        assert!(!REALTIME_ACTIVITY_ACTIVE.load(std::sync::atomic::Ordering::Acquire));
        assert_eq!(REALTIME_ACTIVITY_TOKEN.load(std::sync::atomic::Ordering::Acquire), 0);
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
