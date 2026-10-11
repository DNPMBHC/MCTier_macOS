//! macOS 常驻特权守护进程：一次授权、整个 app 运行期复用。
//!
//! 背景：[`super::macos_privilege`] 每次创建/加入大厅都新起一个 `osascript ...
//! with administrator privileges`，于是每次都弹密码。本模块改为**第一次**授权时
//! 启动一个常驻 root 守护进程（supervisor daemon），之后开/停大厅都通过一个本地
//! 命令 FIFO 把指令发给它，由这个已经是 root 的进程代为启停 easytier-core——
//! 不再弹密码，直到用户完全退出 MCTier。
//!
//! # 安全模型（务必理解）
//!
//! 这刻意用「运行期常驻 root 进程」换取「不重复输密码」。它**降低**了原先
//! 「每次提权都要用户确认」的安全闸，因此做了三重收窄，把本地提权面压到最小：
//!
//! 1. **固定可执行文件**：守护进程只会执行启动它时传入的、锁定的 easytier-core
//!    绝对路径（来自 app 资源目录），命令里带的 `<command>` 字段必须与之逐字节相等，
//!    否则拒绝。无法借它跑任意程序。
//! 2. **参数白名单**：每个 `--flag` 都必须在 [`ALLOWED_FLAGS`] 内；带值的参数值
//!    长度、字符集受限。无法注入 `--script` / shell 元字符等。
//! 3. **一次性 token**：token 在授权那一刻随 argv 直接传给 root 守护进程（不落盘），
//!    每条命令都必须携带它。同用户的其它进程即便能写 FIFO，没有 token 也发不动指令。
//!
//! 它防不住「能读取 MCTier 进程内存拿到 token 的同用户进程」——未签名构建无法
//! 彻底堵这条，真正根治只能靠 Developer ID 签名的 SMAppService helper。这是
//! 用户在知情后明确选择的权衡（见 README 的 Distribution 一节）。
//!
//! # 协议
//!
//! 命令 FIFO 每行一条制表符分隔记录：`token\top\ttag\tcommand\targ1\targ2...`。
//! - `op=start`：以 root 启动 easytier-core（校验 command==锁定路径、args 全部过白名单），
//!   stdout/stderr 重定向到 `<runtime>/<tag>/std{out,err}.fifo`（父进程照常流式读取）。
//! - `op=stop`：终止该 tag 的 easytier-core。
//! - `op=quit`：守护进程自行退出（app 关闭时发送）。
//! 守护进程还会轮询 MCTier 的 PID：app 消失即终止全部子进程并退出，杜绝孤儿 root 进程。

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::macos_privilege::{applescript_escape, create_fifo, open_fifo_reader, shell_quote, FifoReader};

/// 允许传给 root easytier-core 的参数白名单。必须与
/// `network_service::apply_advanced_config` 及基础命令行实际使用的 flag 对齐；
/// 新增 flag 时两边同步，否则该 flag 会被守护进程拒绝、启动失败。
const ALLOWED_FLAGS: &[&str] = &[
    "--network-name",
    "--network-secret",
    "--peers",
    "--hostname",
    "--instance-name",
    "--config-dir",
    "--rpc-portal",
    "--listeners",
    "--default-protocol",
    "--ipv4",
    "--dhcp",
    "--no-tun",
    "--socks5",
    "--port-forward",
    "--proxy-forward-by-system",
    "--proxy-networks",
    "--enable-exit-node",
    "--exit-nodes",
    "--multi-thread",
    "--multi-thread-count",
    "--latency-first",
    "--use-smoltcp",
    "--enable-kcp-proxy",
    "--disable-kcp-input",
    "--enable-quic-proxy",
    "--disable-quic-input",
    "--quic-listen-port",
    "--encryption-algorithm",
    "--bind-device",
    "--dev-name",
    "--mtu",
    "--p2p-only",
    "--disable-p2p",
    "--disable-udp-hole-punching",
    "--disable-tcp-hole-punching",
    "--disable-sym-hole-punching",
    "--enable-relay-foreign-network-kcp",
    "--disable-relay-kcp",
    "--relay-all-peer-rpc",
    "--relay-network-whitelist",
    "--foreign-relay-bps-limit",
    "--manual-routes",
    "--mapped-listeners",
    "--no-listener",
    "--compression",
    "--accept-dns",
    "--private-mode",
    "--tld-dns-zone",
    "--stun-servers",
    "--tcp-whitelist",
    "--udp-whitelist",
    "--proxy-networks",
    "--enable-quic-proxy",
];

/// 单个参数值的最大长度（防畸形超长输入）。
const MAX_ARG_LEN: usize = 512;

/// 一个活动的 EasyTier 会话（daemon 模式）：输出流 + 用于停止的 tag。
pub struct DaemonSession {
    pub tag: String,
    pub stdout: FifoReader,
    pub stderr: FifoReader,
}

/// 守护进程句柄。整个 app 生命周期内最多一个。
struct Daemon {
    /// osascript 子进程（持有 root 守护进程）。保存以便退出时回收。
    process: Child,
    /// 一次性鉴权 token。
    token: String,
    /// 锁定的 easytier-core 绝对路径；start 命令的 command 必须与之相等。
    locked_binary: PathBuf,
    /// 命令 FIFO 的写入端路径。
    command_fifo: PathBuf,
    /// 守护进程根运行目录。
    runtime_root: PathBuf,
}

static DAEMON: Mutex<Option<Daemon>> = Mutex::const_new(None);

/// 守护进程根运行目录（与数据根同源，避免早期版本写进旧目录卡死迁移的问题）。
fn daemon_root() -> PathBuf {
    crate::modules::app_paths::data_root()
        .unwrap_or_else(|_| std::env::temp_dir().join(crate::modules::app_paths::APP_ID))
        .join("privileged-daemon")
}

/// root 侧常驻守护脚本。参数：<token> <app-pid> <command-fifo> <runtime-root> <locked-binary>
///
/// 用 /bin/sh，纯文本协议（每行一条 JSON，由 sh 读取后用内嵌逻辑解析关键字段）。
/// 为避免在 sh 里写 JSON 解析器，这里约定一种**行协议**而非真 JSON：Rust 侧发送
/// 以 \t 分隔的字段，顺序固定：`token\top\ttag\tcommand\targ1\targ2...`。
/// sh 用 IFS=\t 读取，既简单又不受空格/引号困扰。
const DAEMON_SCRIPT: &str = r#"#!/bin/sh
# MCTier macOS 常驻特权守护脚本（由 osascript 以 root 执行，整个 app 运行期常驻）。
# 参数: <token> <app-pid> <command-fifo> <runtime-root> <locked-binary>
set -u

TOKEN="$1"
APP_PID="$2"
CMD_FIFO="$3"
RUNTIME_ROOT="$4"
LOCKED_BIN="$5"

# 后台看门狗：app 进程消失即杀掉所有 easytier-core 子进程并退出，杜绝孤儿 root 进程。
(
  while kill -0 "$APP_PID" 2>/dev/null; do
    sleep 1
  done
  # app 没了：终止所有记录在案的子进程
  for pidfile in "$RUNTIME_ROOT"/*/pid; do
    [ -f "$pidfile" ] || continue
    kill -TERM "$(cat "$pidfile" 2>/dev/null)" 2>/dev/null
  done
  # 让主循环也退出
  echo "WATCHDOG_EXIT" > "$CMD_FIFO" 2>/dev/null || true
) &
WATCHDOG=$!

cleanup_all() {
  for pidfile in "$RUNTIME_ROOT"/*/pid; do
    [ -f "$pidfile" ] || continue
    kill -TERM "$(cat "$pidfile" 2>/dev/null)" 2>/dev/null
  done
  kill -TERM "$WATCHDOG" 2>/dev/null
}
trap cleanup_all EXIT

# 主循环：持续从命令 FIFO 读行。FIFO 在所有写者关闭时会读到 EOF，用外层 while 重开。
while kill -0 "$APP_PID" 2>/dev/null; do
  while IFS=$(printf '\t') read -r C_TOKEN C_OP C_TAG C_CMD C_REST; do
    # 鉴权：token 不符直接忽略（防同用户其它进程伪造指令）。
    [ "$C_TOKEN" = "$TOKEN" ] || continue

    case "$C_OP" in
      quit)
        exit 0
        ;;
      stop)
        PIDFILE="$RUNTIME_ROOT/$C_TAG/pid"
        if [ -f "$PIDFILE" ]; then
          kill -TERM "$(cat "$PIDFILE" 2>/dev/null)" 2>/dev/null
          rm -f "$PIDFILE"
        fi
        ;;
      start)
        # 固定可执行文件校验：command 字段必须与锁定路径逐字节相等。
        [ "$C_CMD" = "$LOCKED_BIN" ] || continue
        SESS_DIR="$RUNTIME_ROOT/$C_TAG"
        OUT_FIFO="$SESS_DIR/stdout.fifo"
        ERR_FIFO="$SESS_DIR/stderr.fifo"
        [ -p "$OUT_FIFO" ] || continue
        [ -p "$ERR_FIFO" ] || continue
        # C_REST 是制表符分隔的参数，用 IFS 还原成位置参数。
        OLD_IFS="$IFS"
        IFS=$(printf '\t')
        # shellcheck disable=SC2086
        set -- $C_REST
        IFS="$OLD_IFS"
        "$LOCKED_BIN" "$@" >"$OUT_FIFO" 2>"$ERR_FIFO" &
        echo $! > "$SESS_DIR/pid"
        ;;
    esac
  done < "$CMD_FIFO"
done
exit 0
"#;

/// token 字符集：仅十六进制，确保能安全放进制表符行协议且不含分隔符。
fn new_token() -> String {
    let mut bytes = [0u8; 24];
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// 校验单个参数：非空、长度受限、不含制表符/换行/控制字符（行协议分隔符）。
fn arg_is_safe(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ARG_LEN
        && !value.chars().any(|c| c == '\t' || c == '\n' || c == '\r' || c.is_control())
}

/// 校验整条参数序列是否全部由白名单 flag 与安全值组成。
///
/// 规则：以 `--` 开头的 token 必须在白名单内；其余 token 视为「上一个 flag 的值」，
/// 做安全性校验。第一个 token 必须是 flag。
fn validate_args(args: &[String]) -> Result<(), String> {
    for arg in args {
        if let Some(_flag) = arg.strip_prefix("--").map(|_| arg.as_str()) {
            if !ALLOWED_FLAGS.contains(&arg.as_str()) {
                return Err(format!("参数不在白名单内，拒绝提权执行: {arg}"));
            }
        } else if !arg_is_safe(arg) {
            return Err("参数值包含非法字符或过长，拒绝提权执行".to_string());
        }
    }
    Ok(())
}

/// 守护进程是否已在运行。
pub async fn is_running() -> bool {
    let mut guard = DAEMON.lock().await;
    if let Some(daemon) = guard.as_mut() {
        // osascript 还活着就认为可用；已退出则清掉句柄，下次重新授权。
        match daemon.process.try_wait() {
            Ok(None) => true,
            _ => {
                *guard = None;
                false
            }
        }
    } else {
        false
    }
}

/// 启动常驻守护进程（弹一次授权窗口）。幂等：已在运行则直接返回。
pub async fn ensure_daemon(easytier: &Path) -> Result<(), String> {
    let mut guard = DAEMON.lock().await;
    if let Some(daemon) = guard.as_mut() {
        if matches!(daemon.process.try_wait(), Ok(None)) {
            return Ok(());
        }
        *guard = None;
    }

    let runtime_root = daemon_root();
    // 清掉上次残留目录（内含旧 FIFO/pid），重新建一个 0700 的根。
    let _ = std::fs::remove_dir_all(&runtime_root);
    std::fs::create_dir_all(&runtime_root)
        .map_err(|e| format!("创建守护进程目录失败: {e}"))?;
    std::fs::set_permissions(&runtime_root, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("设置守护进程目录权限失败: {e}"))?;

    let command_fifo = runtime_root.join("command.fifo");
    let _ = std::fs::remove_file(&command_fifo);
    create_fifo(&command_fifo)?;

    let script_path = runtime_root.join("daemon.sh");
    std::fs::write(&script_path, DAEMON_SCRIPT)
        .map_err(|e| format!("写入守护脚本失败: {e}"))?;
    std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("设置守护脚本权限失败: {e}"))?;

    let token = new_token();
    let locked_binary = easytier.to_path_buf();

    // token 随 argv 直接传给 root 守护进程，不落盘。
    let parts = vec![
        shell_quote(&script_path.to_string_lossy()),
        shell_quote(&token),
        std::process::id().to_string(),
        shell_quote(&command_fifo.to_string_lossy()),
        shell_quote(&runtime_root.to_string_lossy()),
        shell_quote(&locked_binary.to_string_lossy()),
    ];
    let command = parts.join(" ");
    let script = format!(
        "do shell script \"{}\" with administrator privileges",
        applescript_escape(&command)
    );

    let process = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("无法启动 macOS 管理员授权窗口: {e}"))?;

    log::info!("macOS：已请求管理员授权，启动常驻特权守护进程（运行期复用，不再重复弹密码）");

    *guard = Some(Daemon {
        process,
        token,
        locked_binary,
        command_fifo,
        runtime_root,
    });
    Ok(())
}

/// 通过守护进程以 root 启动一个 EasyTier 会话。调用方须先 [`ensure_daemon`]。
pub async fn start_session(
    easytier: &Path,
    args: &[String],
    tag: &str,
) -> Result<DaemonSession, String> {
    // 校验参数白名单（在拿锁之前，失败不影响守护进程状态）。
    validate_args(args)?;
    if tag.is_empty() || !tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("非法会话标识".to_string());
    }

    let mut guard = DAEMON.lock().await;
    let daemon = guard.as_mut().ok_or("守护进程未运行")?;

    // 固定可执行文件：传入路径必须与守护进程锁定的一致。
    if easytier != daemon.locked_binary {
        return Err("EasyTier 路径与守护进程锁定的不一致，拒绝启动".to_string());
    }

    let session_dir = daemon.runtime_root.join(tag);
    std::fs::create_dir_all(&session_dir)
        .map_err(|e| format!("创建会话目录失败: {e}"))?;
    std::fs::set_permissions(&session_dir, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("设置会话目录权限失败: {e}"))?;

    let out_fifo = session_dir.join("stdout.fifo");
    let err_fifo = session_dir.join("stderr.fifo");
    for fifo in [&out_fifo, &err_fifo] {
        let _ = std::fs::remove_file(fifo);
        create_fifo(fifo)?;
    }
    // 读取端必须先就绪：root 侧重定向会在打开 FIFO 时阻塞直到有读者。
    let stdout = open_fifo_reader(&out_fifo)?;
    let stderr = open_fifo_reader(&err_fifo)?;

    // 组装制表符行协议：token\tstart\ttag\tcommand\targ1\targ2...
    let mut fields = vec![
        daemon.token.clone(),
        "start".to_string(),
        tag.to_string(),
        daemon.locked_binary.to_string_lossy().to_string(),
    ];
    fields.extend(args.iter().cloned());
    let line = format!("{}\n", fields.join("\t"));

    write_command(&daemon.command_fifo, &line)?;
    log::info!("已通过常驻守护进程启动 EasyTier 会话（tag={tag}，无需重新授权）");

    Ok(DaemonSession {
        tag: tag.to_string(),
        stdout,
        stderr,
    })
}

/// 请求守护进程停止某个会话。
pub async fn stop_session(tag: &str) -> Result<(), String> {
    let guard = DAEMON.lock().await;
    let Some(daemon) = guard.as_ref() else {
        return Ok(());
    };
    let line = format!("{}\tstop\t{}\n", daemon.token, tag);
    write_command(&daemon.command_fifo, &line)?;
    // 会话目录由守护进程留着 pid 文件，这里顺手清理读取端目录（best-effort）。
    let session_dir = daemon.runtime_root.join(tag);
    let _ = std::fs::remove_dir_all(&session_dir);
    Ok(())
}

/// 停止所有活动会话（不需要知道具体 tag）。供「取消连接」这类不持服务锁的路径使用。
///
/// 直接扫描运行目录下所有会话子目录，逐个给守护进程发 stop 指令。
pub async fn stop_all_sessions() -> usize {
    let guard = DAEMON.lock().await;
    let Some(daemon) = guard.as_ref() else {
        return 0;
    };
    let Ok(entries) = std::fs::read_dir(&daemon.runtime_root) else {
        return 0;
    };
    let mut stopped = 0;
    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(tag) = name.to_str() else { continue };
        // 只停形如 mctier-* 的会话目录，别碰 command.fifo 等文件。
        if !tag.starts_with("mctier-") {
            continue;
        }
        let line = format!("{}\tstop\t{}\n", daemon.token, tag);
        if write_command(&daemon.command_fifo, &line).is_ok() {
            let _ = std::fs::remove_dir_all(entry.path());
            stopped += 1;
        }
    }
    if stopped > 0 {
        log::info!("已请求守护进程停止 {stopped} 个 macOS EasyTier 会话", );
    }
    stopped
}

/// 关闭守护进程（app 退出时调用）。发送 quit 并回收 osascript。
pub async fn shutdown_daemon() {
    let mut guard = DAEMON.lock().await;
    if let Some(mut daemon) = guard.take() {
        let line = format!("{}\tquit\n", daemon.token);
        let _ = write_command(&daemon.command_fifo, &line);
        // 给守护进程一点时间自行退出，然后兜底回收。
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), daemon.process.wait()).await;
        let _ = daemon.process.start_kill();
        let _ = std::fs::remove_dir_all(&daemon.runtime_root);
        log::info!("macOS 常驻特权守护进程已关闭");
    }
}

/// 向命令 FIFO 写一行。阻塞式打开写端（守护进程常驻、读端始终存在，不会卡）。
fn write_command(fifo: &Path, line: &str) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(fifo)
        .map_err(|e| format!("打开命令通道失败: {e}"))?;
    file.write_all(line.as_bytes())
        .map_err(|e| format!("写入命令通道失败: {e}"))?;
    file.flush().map_err(|e| format!("刷新命令通道失败: {e}"))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_rejects_unknown_flags() {
        assert!(validate_args(&["--network-name".into(), "lobby".into()]).is_ok());
        assert!(validate_args(&["--rpc-portal".into(), "127.0.0.1:1234".into()]).is_ok());
        // 伪造一个不在白名单里的危险参数：必须拒绝。
        assert!(validate_args(&["--exec".into(), "/bin/sh".into()]).is_err());
        assert!(validate_args(&["--script".into()]).is_err());
    }

    #[test]
    fn arg_values_reject_protocol_delimiters_and_overlong() {
        // 制表符/换行是行协议分隔符，绝不能出现在值里。
        assert!(!arg_is_safe("has\ttab"));
        assert!(!arg_is_safe("has\nnewline"));
        assert!(!arg_is_safe(""));
        assert!(!arg_is_safe(&"x".repeat(MAX_ARG_LEN + 1)));
        assert!(arg_is_safe("tcp://node.example.com:11010"));
        assert!(arg_is_safe("10.126.126.5/24"));
    }

    #[test]
    fn validate_args_flags_a_tab_injected_value() {
        // 大厅名里塞制表符想截断协议：必须被值校验挡下。
        let args = vec!["--network-name".to_string(), "evil\tfield".to_string()];
        assert!(validate_args(&args).is_err());
    }

    #[test]
    fn token_is_hex_and_unique() {
        let a = new_token();
        let b = new_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 48);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// 白名单必须覆盖基础命令行用到的全部 flag，否则 daemon 启动会把它们判成非法。
    #[test]
    fn whitelist_covers_base_command_flags() {
        for flag in [
            "--network-name",
            "--network-secret",
            "--peers",
            "--hostname",
            "--instance-name",
            "--config-dir",
            "--rpc-portal",
            "--listeners",
            "--default-protocol",
            "--encryption-algorithm",
        ] {
            assert!(
                ALLOWED_FLAGS.contains(&flag),
                "基础命令行 flag {flag} 必须在白名单内"
            );
        }
    }
}
